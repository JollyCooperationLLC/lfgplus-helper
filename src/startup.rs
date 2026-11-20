//! Where the helper may register itself to start with the computer.
//!
//! On macOS a downloaded app that hasn't been moved yet runs from a temporary, read-only copy (App Translocation), and
//! one opened straight from a disk image runs from the image. Either path is gone after a restart, so starting with
//! the computer from there would quietly stop working. The helper asks to be moved to Applications instead.

use std::path::Path;

/// True when the program at `exe` runs from somewhere that won't be there after a restart: a translocated copy or a
/// mounted disk image.
pub fn must_move(exe: &Path) -> bool {
    let text = exe.to_string_lossy();
    text.contains("/AppTranslocation/") || text.starts_with("/Volumes/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_to_move_from_temporary_places() {
        assert!(must_move(Path::new(
            "/private/var/folders/xy/abc/T/AppTranslocation/1234-ABCD/d/LFG+ Helper.app/Contents/MacOS/lfgplus-helper"
        )));
        assert!(must_move(Path::new("/Volumes/LFG+ Helper/LFG+ Helper.app/Contents/MacOS/lfgplus-helper")));
    }

    #[test]
    fn starts_from_where_it_was_installed() {
        assert!(!must_move(Path::new("/Applications/LFG+ Helper.app/Contents/MacOS/lfgplus-helper")));
        assert!(!must_move(Path::new("/Users/me/Applications/LFG+ Helper.app/Contents/MacOS/lfgplus-helper")));
        assert!(!must_move(Path::new("C:\\Users\\me\\Desktop\\lfgplus-helper.exe")));
        assert!(!must_move(Path::new("/home/me/bin/lfgplus-helper")));
    }
}
