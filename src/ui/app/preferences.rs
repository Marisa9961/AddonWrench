use super::columns::ColumnWidths;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub(super) struct Preferences {
    pub author_column: bool,
    pub modified_column: bool,
    pub column_widths: ColumnWidths,
}

#[derive(Clone, Copy)]
pub(super) enum OptionalColumn {
    Author,
    Modified,
}

impl Preferences {
    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|error| format!("显示设置损坏：{error}"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("无法读取显示设置：{error}")),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let lock_path = path.with_extension("lock");
        let lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|error| format!("无法保存显示设置：{error}"))?;
        let result = (|| {
            // Preserve malformed settings for recovery instead of overwriting them.
            Self::load(path)?;
            let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
            let temp = path.with_extension("tmp");
            fs::write(&temp, bytes).map_err(|error| error.to_string())?;
            fs::rename(&temp, path).map_err(|error| error.to_string())
        })();
        drop(lock);
        let _ = fs::remove_file(lock_path);
        result
    }

    pub fn visible(self, wide: bool) -> Self {
        Self {
            author_column: self.author_column && wide,
            modified_column: self.modified_column && wide,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_columns_default_hidden_and_old_settings_allow_missing_fields() {
        assert_eq!(
            serde_json::from_str::<Preferences>("{}").unwrap(),
            Preferences::default()
        );
        let old = serde_json::from_str::<Preferences>(r#"{"author_column":true}"#).unwrap();
        assert!(old.author_column);
        assert_eq!(old.column_widths, ColumnWidths::default());
        let prefs = Preferences {
            author_column: true,
            modified_column: false,
            ..Default::default()
        };
        assert_eq!(prefs.visible(false), Preferences::default());
        assert_eq!(prefs.visible(true), prefs);
        assert_eq!(
            serde_json::from_str::<Preferences>(&serde_json::to_string(&prefs).unwrap()).unwrap(),
            prefs
        );
    }

    #[test]
    fn saves_columns_and_preserves_corrupt_settings_and_existing_locks() {
        let directory =
            std::env::temp_dir().join(format!("addon-wrench-columns-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("preferences.json");
        assert_eq!(Preferences::load(&path).unwrap(), Preferences::default());
        let prefs: Preferences = serde_json::from_str(
            r#"{
            "author_column": true, "modified_column": true,
            "column_widths": {"author": 220, "size": 90, "modified": 170, "status": 120}
        }"#,
        )
        .unwrap();
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), prefs);
        fs::write(path.with_extension("lock"), b"locked").unwrap();
        assert!(Preferences::default().save(&path).is_err());
        assert_eq!(Preferences::load(&path).unwrap(), prefs);
        fs::remove_file(path.with_extension("lock")).unwrap();
        fs::write(&path, b"damaged").unwrap();
        assert!(prefs.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"damaged");
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
