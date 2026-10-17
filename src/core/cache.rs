//! Disposable SQLite cache. Windows ships SQLite in winsqlite3.dll (Windows 10+).
//! Bindings stay private; statement buffers outlive sqlite3_step and finalization.
use std::{
    collections::HashSet,
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::Path,
    ptr,
};

#[cfg_attr(windows, link(name = "winsqlite3"))]
#[cfg_attr(not(windows), link(name = "sqlite3"))]
unsafe extern "C" {
    fn sqlite3_open(filename: *const c_char, db: *mut *mut c_void) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
    fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
    fn sqlite3_errmsg(db: *mut c_void) -> *const c_char;
    fn sqlite3_prepare_v2(
        db: *mut c_void,
        sql: *const c_char,
        n: c_int,
        statement: *mut *mut c_void,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_bind_blob(
        statement: *mut c_void,
        index: c_int,
        data: *const c_void,
        size: c_int,
        destructor: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_step(statement: *mut c_void) -> c_int;
    fn sqlite3_column_blob(statement: *mut c_void, column: c_int) -> *const c_void;
    fn sqlite3_column_bytes(statement: *mut c_void, column: c_int) -> c_int;
    fn sqlite3_finalize(statement: *mut c_void) -> c_int;
}

pub struct Cache(*mut c_void);
struct Statement(*mut c_void);
impl Drop for Cache {
    fn drop(&mut self) {
        unsafe {
            sqlite3_close(self.0);
        }
    }
}
impl Drop for Statement {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.0);
        }
    }
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let filename =
            CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let mut db = ptr::null_mut();
        let status = unsafe { sqlite3_open(filename.as_ptr(), &mut db) };
        if db.is_null() {
            return Err("无法打开 SQLite 缓存".into());
        }
        let cache = Self(db);
        if status != 0 {
            return Err(cache.error());
        }
        unsafe {
            sqlite3_busy_timeout(db, 500);
        }
        cache.execute("CREATE TABLE IF NOT EXISTS entries (key BLOB PRIMARY KEY, fingerprint BLOB NOT NULL, payload BLOB NOT NULL, cover BLOB NOT NULL)", &[])?;
        Ok(cache)
    }

    pub fn default_path() -> std::path::PathBuf {
        crate::discovery::data_dir().join("cache.sqlite3")
    }

    fn error(&self) -> String {
        unsafe {
            CStr::from_ptr(sqlite3_errmsg(self.0))
                .to_string_lossy()
                .into_owned()
        }
    }

    // SQLITE_STATIC is safe here: params remain borrowed until Statement is dropped.
    fn statement(&self, sql: &str, params: &[&[u8]]) -> Result<Statement, String> {
        let sql = CString::new(sql).map_err(|e| e.to_string())?;
        let mut raw = ptr::null_mut();
        let status =
            unsafe { sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut raw, ptr::null_mut()) };
        let statement = Statement(raw);
        if status != 0 {
            return Err(self.error());
        }
        for (i, value) in params.iter().enumerate() {
            let size = c_int::try_from(value.len()).map_err(|_| "缓存记录过大")?;
            if unsafe { sqlite3_bind_blob(raw, i as c_int + 1, value.as_ptr().cast(), size, None) }
                != 0
            {
                return Err(self.error());
            }
        }
        Ok(statement)
    }

    fn execute(&self, sql: &str, params: &[&[u8]]) -> Result<(), String> {
        let statement = self.statement(sql, params)?;
        if unsafe { sqlite3_step(statement.0) } == 101 {
            Ok(())
        } else {
            Err(self.error())
        }
    }

    pub fn get(&self, key: &str, fingerprint: &str) -> Option<(Vec<u8>, Vec<u8>)> {
        let statement = self
            .statement(
                "SELECT payload, cover FROM entries WHERE key=? AND fingerprint=?",
                &[key.as_bytes(), fingerprint.as_bytes()],
            )
            .ok()?;
        if unsafe { sqlite3_step(statement.0) } != 100 {
            return None;
        }
        let column = |index| unsafe {
            let len = sqlite3_column_bytes(statement.0, index);
            let data = sqlite3_column_blob(statement.0, index);
            if data.is_null() || len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(data.cast::<u8>(), len as usize).to_vec()
            }
        };
        Some((column(0), column(1)))
    }

    pub fn put(
        &self,
        key: &str,
        fingerprint: &str,
        payload: &[u8],
        cover: &[u8],
    ) -> Result<(), String> {
        self.execute(
            "INSERT OR REPLACE INTO entries VALUES (?, ?, ?, ?)",
            &[key.as_bytes(), fingerprint.as_bytes(), payload, cover],
        )
    }

    pub fn begin(&self) -> Result<(), String> {
        self.execute("BEGIN IMMEDIATE", &[])
    }
    pub fn commit(&self) -> Result<(), String> {
        self.execute("COMMIT", &[])
    }

    pub fn rollback(&self) -> Result<(), String> {
        self.execute("ROLLBACK", &[])
    }

    fn keys(&self, sql: &str, params: &[&[u8]]) -> Result<Vec<Vec<u8>>, String> {
        let statement = self.statement(sql, params)?;
        let mut keys = Vec::new();
        loop {
            match unsafe { sqlite3_step(statement.0) } {
                100 => {
                    let len = unsafe { sqlite3_column_bytes(statement.0, 0) };
                    let data = unsafe { sqlite3_column_blob(statement.0, 0) };
                    if len > 0 && !data.is_null() {
                        keys.push(
                            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) }
                                .to_vec(),
                        );
                    }
                }
                101 => return Ok(keys),
                _ => return Err(self.error()),
            }
        }
    }

    /// Caller holds a transaction and has completed the entire directory scan.
    /// Only confirmed missing files inside this game's path may be removed.
    pub fn prune_missing_local(&self, game: &Path, live: &HashSet<String>) -> Result<(), String> {
        for bytes in self.keys("SELECT key FROM entries", &[])? {
            let Ok(key) = std::str::from_utf8(&bytes) else {
                continue;
            };
            let Some(path) = key.strip_prefix("local:").map(Path::new) else {
                continue;
            };
            if path.starts_with(game)
                && !live.contains(key)
                && matches!(path.try_exists(), Ok(false))
            {
                self.execute("DELETE FROM entries WHERE key=?", &[&bytes])?;
            }
        }
        Ok(())
    }

    /// References from other installation directories keep shared workshop data alive.
    /// Existing databases receive the full grace period on their first observation.
    pub fn maintain_retained(
        &self,
        scope: &str,
        fingerprint: &str,
        live: &[String],
        now: u64,
        grace: u64,
    ) -> Result<(), String> {
        self.begin()?;
        let result = (|| {
            self.execute("CREATE TABLE IF NOT EXISTS cache_references (scope BLOB NOT NULL, key BLOB NOT NULL, PRIMARY KEY(scope, key))", &[])?;
            self.execute("CREATE TABLE IF NOT EXISTS cache_orphans (key BLOB PRIMARY KEY, since INTEGER NOT NULL)", &[])?;
            self.execute(
                "DELETE FROM cache_references WHERE scope=?",
                &[scope.as_bytes()],
            )?;
            for key in live {
                self.execute(
                    "INSERT OR IGNORE INTO cache_references VALUES (?, ?)",
                    &[scope.as_bytes(), key.as_bytes()],
                )?;
            }
            // A reinstall cancels the pending cleanup, even at the grace boundary.
            self.execute(
                "DELETE FROM cache_orphans WHERE key IN (SELECT key FROM cache_references)",
                &[],
            )?;
            let stamp = now.to_string();
            self.execute("INSERT OR IGNORE INTO cache_orphans SELECT key, CAST(? AS INTEGER) FROM entries e WHERE fingerprint=? AND NOT EXISTS (SELECT 1 FROM cache_references r WHERE r.key=e.key)", &[stamp.as_bytes(), fingerprint.as_bytes()])?;
            if let Some(cutoff) = now.checked_sub(grace) {
                let cutoff = cutoff.to_string();
                self.execute("DELETE FROM entries WHERE fingerprint=? AND key IN (SELECT key FROM cache_orphans WHERE since<=CAST(? AS INTEGER)) AND NOT EXISTS (SELECT 1 FROM cache_references r WHERE r.key=entries.key)", &[fingerprint.as_bytes(), cutoff.as_bytes()])?;
            }
            self.execute(
                "DELETE FROM cache_orphans WHERE key NOT IN (SELECT key FROM entries)",
                &[],
            )?;
            self.commit()
        })();
        if result.is_err() {
            let _ = self.rollback();
        }
        result
    }
}

/// Include preview sidecars and external VPK chunks, including added/removed files.
pub fn fingerprint(path: &Path) -> Option<String> {
    let stamp = |p: &Path| -> Option<String> {
        match std::fs::metadata(p) {
            Ok(m) => Some(format!(
                "{}:{}",
                m.len(),
                m.modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_nanos()
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some("missing".into()),
            Err(_) => None,
        }
    };
    let mut result = format!("local-v3:{}", stamp(path)?);
    for extension in ["jpg", "png", "jpeg", "gif", "webp"] {
        result.push_str(&format!(
            ":{extension}:{}",
            stamp(&path.with_extension(extension))?
        ));
    }
    if let Some(stem) = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_suffix("_dir"))
    {
        let prefix = format!("{stem}_");
        let mut chunks = Vec::new();
        for entry in std::fs::read_dir(path.parent()?).ok()? {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(index) = name
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".vpk"))
                && index.len() == 3
                && index.bytes().all(|b| b.is_ascii_digit())
            {
                chunks.push(format!(":{name}:{}", stamp(&entry.path())?));
            }
        }
        chunks.sort();
        result.extend(chunks);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_cache_grace_reinstall_and_game_isolation() {
        let cache = Cache::open(Path::new(":memory:")).unwrap();
        cache
            .put("deleted", "app-a", b"metadata", b"cover")
            .unwrap();
        cache.put("other", "app-b", b"other", b"cover").unwrap();
        cache
            .maintain_retained("game-a", "app-a", &[], 100, 30)
            .unwrap();
        cache
            .maintain_retained("game-a", "app-a", &[], 129, 30)
            .unwrap();
        assert!(cache.get("deleted", "app-a").is_some());
        cache
            .maintain_retained("game-a", "app-a", &["deleted".into()], 130, 30)
            .unwrap();
        assert!(cache.get("deleted", "app-a").is_some());
        cache
            .maintain_retained("game-a", "app-a", &[], 140, 30)
            .unwrap();
        cache
            .maintain_retained("game-a", "app-a", &[], 169, 30)
            .unwrap();
        assert!(cache.get("deleted", "app-a").is_some());
        cache
            .maintain_retained("game-a", "app-a", &[], 170, 30)
            .unwrap();
        assert!(cache.get("deleted", "app-a").is_none());
        assert!(cache.get("other", "app-b").is_some());
    }

    #[test]
    fn another_installation_keeps_shared_workshop_cache_alive() {
        let cache = Cache::open(Path::new(":memory:")).unwrap();
        cache.put("shared", "app-a", b"metadata", b"cover").unwrap();
        cache
            .maintain_retained("install-a", "app-a", &["shared".into()], 100, 30)
            .unwrap();
        cache
            .maintain_retained("install-b", "app-a", &["shared".into()], 100, 30)
            .unwrap();
        cache
            .maintain_retained("install-a", "app-a", &[], 200, 30)
            .unwrap();
        cache
            .maintain_retained("install-a", "app-a", &[], 300, 30)
            .unwrap();
        assert!(cache.get("shared", "app-a").is_some());
        cache
            .maintain_retained("install-b", "app-a", &[], 300, 30)
            .unwrap();
        cache
            .maintain_retained("install-b", "app-a", &[], 330, 30)
            .unwrap();
        assert!(cache.get("shared", "app-a").is_none());
    }
}
