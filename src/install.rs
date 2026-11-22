//! On Windows and Linux the helper installs itself: a plain start from anywhere else (usually the Downloads folder)
//! copies the program to the user's own programs folder and starts it from there. Starting with the computer then
//! points at a copy that stays put, the download can be deleted, and opening a newer download replaces it, which is
//! how the helper is updated. The Mac app is installed by dragging it to Applications instead (`startup.rs`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Where the helper lives once installed: `%LOCALAPPDATA%\Programs\LFG+ Helper\lfgplus-helper.exe` on Windows,
/// `~/.local/bin/lfgplus-helper` on Linux. None on macOS.
#[cfg(windows)]
pub fn install_path() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("Programs").join("LFG+ Helper").join("lfgplus-helper.exe"))
}

#[cfg(target_os = "macos")]
pub fn install_path() -> Option<PathBuf> {
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn install_path() -> Option<PathBuf> {
    Some(dirs::executable_dir()?.join("lfgplus-helper"))
}

/// True when the program at `exe` isn't the installed copy at `target`.
pub fn needs_install(exe: &Path, target: &Path) -> bool {
    match (fs::canonicalize(exe), fs::canonicalize(target)) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    }
}

/// Copies `exe` to `target`, replacing what's there. The copy is written beside it and renamed over it, so a failure
/// leaves the old one whole.
pub fn install(exe: &Path, target: &Path) -> io::Result<()> {
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = target.with_extension("new");
    fs::copy(exe, &temp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755))?;
    }
    fs::rename(&temp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_into_the_users_programs() {
        let path = install_path();
        if cfg!(target_os = "macos") {
            assert_eq!(path, None);
        } else {
            let path = path.expect("a programs folder");
            assert!(path.to_string_lossy().contains("lfgplus-helper"));
        }
    }

    #[test]
    fn copies_itself_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        let download = dir.path().join("Downloads").join("lfgplus-helper-windows.exe");
        fs::create_dir_all(download.parent().expect("parent")).expect("mkdir");
        fs::write(&download, "new program").expect("download");
        let target = dir.path().join("Programs").join("LFG+ Helper").join("lfgplus-helper.exe");

        assert!(needs_install(&download, &target), "nothing installed yet");
        install(&download, &target).expect("installs");
        assert_eq!(fs::read_to_string(&target).expect("installed"), "new program");
        assert!(!target.with_extension("new").exists());
        assert!(needs_install(&download, &target), "the download still isn't the installed copy");
        assert!(!needs_install(&target, &target), "the installed copy runs from where it belongs");

        // A newer download replaces it.
        fs::write(&download, "newer program").expect("download");
        install(&download, &target).expect("installs again");
        assert_eq!(fs::read_to_string(&target).expect("installed"), "newer program");
    }

    #[test]
    fn reports_what_it_couldnt_do() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("target.exe");
        assert!(install(&dir.path().join("missing.exe"), &target).is_err());
        fs::write(dir.path().join("file"), "x").expect("file");
        assert!(install(&target, &dir.path().join("file").join("x.exe")).is_err());
    }
}
