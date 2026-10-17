//! Detect the selected L4D2 installation, including launches outside the manager.
use std::path::Path;

pub const RUNNING_MESSAGE: &str = "游戏运行中，暂时无法修改模组";

pub fn ensure_stopped(game: &Path) -> Result<(), String> {
    if is_running(game)? {
        Err(RUNNING_MESSAGE.into())
    } else {
        Ok(())
    }
}

pub fn is_running(game: &Path) -> Result<bool, String> {
    platform::is_running(game)
        .map_err(|error| format!("无法确认游戏运行状态，暂时禁止修改模组：{error}"))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{ffi::OsString, mem, os::windows::ffi::OsStringExt, path::PathBuf};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, HANDLE,
            INVALID_HANDLE_VALUE, STILL_ACTIVE,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn image_path(pid: u32) -> Result<Option<PathBuf>, String> {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            let error = std::io::Error::last_os_error();
            // A process may have exited between enumeration and opening its handle.
            return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                Ok(None)
            } else {
                Err(error.to_string())
            };
        }
        let process = Handle(process);
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) }
            == 0
        {
            let error = std::io::Error::last_os_error();
            let mut exit_code = 0;
            if unsafe { GetExitCodeProcess(process.0, &mut exit_code) } != 0
                && exit_code != STILL_ACTIVE as u32
            {
                return Ok(None);
            }
            return Err(error.to_string());
        }
        Ok(Some(PathBuf::from(OsString::from_wide(
            &buffer[..length as usize],
        ))))
    }

    pub(super) fn is_running(game: &Path) -> Result<bool, String> {
        let install = game.parent().ok_or("无法确定游戏安装目录")?;
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let snapshot = Handle(snapshot);
        // All fields are plain integers/arrays; ToolHelp requires dwSize before the first call.
        let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = unsafe { Process32FirstW(snapshot.0, &mut entry) };
        let mut unresolved = None;
        while found != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if executable_name(&name) {
                match image_path(entry.th32ProcessID) {
                    Ok(Some(path)) if matches_installation(install, &path) => return Ok(true),
                    Ok(_) => {}
                    Err(error) => unresolved = Some(error),
                }
            }
            found = unsafe { Process32NextW(snapshot.0, &mut entry) };
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
            return Err(error.to_string());
        }
        unresolved.map_or(Ok(false), Err)
    }

    fn executable_name(name: &str) -> bool {
        name.eq_ignore_ascii_case("left4dead2.exe")
            || name.eq_ignore_ascii_case("left4dead2_x64.exe")
    }

    fn matches_installation(install: &Path, image: &Path) -> bool {
        let Some(name) = image.file_name().and_then(|s| s.to_str()) else {
            return false;
        };
        if !executable_name(name) {
            return false;
        }
        let expected = install.join(name);
        // Canonicalization handles junctions, short paths, and extended-length prefixes.
        let expected = expected.canonicalize().unwrap_or(expected);
        let actual = image.canonicalize().unwrap_or_else(|_| image.into());
        expected
            .as_os_str()
            .eq_ignore_ascii_case(actual.as_os_str())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn matches_only_the_selected_installation_and_game_executable() {
            let install = Path::new(r"C:\Games\L4D2");
            assert!(matches_installation(
                install,
                Path::new(r"c:\games\l4d2\LEFT4DEAD2.EXE")
            ));
            assert!(matches_installation(
                install,
                Path::new(r"C:\Games\L4D2\left4dead2_x64.exe")
            ));
            for other in [
                r"C:\Other\left4dead2.exe",
                r"C:\Games\L4D2 copy\left4dead2.exe",
                r"C:\Games\L4D2\bin\left4dead2.exe",
                r"C:\Games\L4D2\launcher.exe",
            ] {
                assert!(!matches_installation(install, Path::new(other)));
            }
        }

        // The ordinary test executable acts as a controllable game process in an isolated install.
        #[test]
        fn process_fixture() {
            if std::env::var_os("ADDON_WRENCH_PROCESS_FIXTURE").is_some() {
                use std::io::Read;
                let _ = std::io::stdin().read(&mut [0u8]);
            }
        }

        #[test]
        fn running_process_blocks_real_writes_and_imports_only_for_its_installation() {
            use std::{
                fs,
                os::windows::process::CommandExt,
                process::{Command, Stdio},
            };
            let root = std::env::temp_dir().join(format!("aw-runtime-{}", std::process::id()));
            let game = root.join("left4dead2");
            fs::create_dir_all(game.join("addons")).unwrap();
            let executable = root.join("left4dead2.exe");
            fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
            let mut child = Command::new(&executable)
                .args([
                    "--exact",
                    "core::runtime::platform::tests::process_fixture",
                    "--include-ignored",
                ])
                .env("ADDON_WRENCH_PROCESS_FIXTURE", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn()
                .unwrap();
            let result = std::panic::catch_unwind(|| {
                assert!(is_running(&game).unwrap());
                assert!(!is_running(&root.join("other/left4dead2")).unwrap());
                let config = game.join("addonlist.txt");
                let original = b"\"AddonList\" { \"example.vpk\" \"1\" }";
                fs::write(&config, original).unwrap();
                assert_eq!(
                    crate::addons::set_enabled(&game, &[("example.vpk".into(), false)])
                        .unwrap_err(),
                    RUNNING_MESSAGE
                );
                assert_eq!(fs::read(&config).unwrap(), original);
                assert_eq!(
                    crate::addons::import(&game, &root.join("example.vpk")).unwrap_err(),
                    RUNNING_MESSAGE
                );
                assert_eq!(fs::read_dir(game.join("addons")).unwrap().count(), 0);
                assert!(!game.join(".addonwrench.lock").exists());
            });
            drop(child.stdin.take());
            child.wait().unwrap();
            let stopped = is_running(&game).unwrap();
            let resumed = if result.is_ok() {
                crate::addons::set_enabled(&game, &[("example.vpk".into(), false)])
            } else {
                Ok(())
            };
            fs::remove_dir_all(root).unwrap();
            assert!(!stopped);
            resumed.unwrap();
            if let Err(error) = result {
                std::panic::resume_unwind(error);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn is_running(_: &Path) -> Result<bool, String> {
        Err("当前平台不支持游戏运行检测".into())
    }
}
