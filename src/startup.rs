//! When and how the helper registers itself to start with the computer.
//!
//! Only a player's own start registers anything. A build someone is working on, or a run with options (other settings,
//! another site), would otherwise point the startup entry at itself, and starting with the computer would stop working
//! once that program was gone.
//!
//! On macOS a downloaded app that hasn't been moved yet runs from a temporary, read-only copy (App Translocation), and
//! one opened straight from a disk image runs from the image. Either path is gone after a restart, so starting with
//! the computer from there would quietly stop working. The helper asks to be moved to Applications instead.

use std::path::Path;

use crate::update;

/// True for a player's own start: a released build, opened with no options. Only then does the helper install itself
/// and change what starts with the computer.
pub fn players_start(plain: bool, version: &str) -> bool {
    plain && update::parse(version).is_some()
}

/// The program's path as this system's startup entry needs it. Windows and Linux store a command line, where a path
/// with a space in it must be quoted. macOS stores the path by itself.
pub fn entry(exe: &str) -> String {
    if cfg!(windows) {
        windows_entry(exe)
    } else if cfg!(target_os = "macos") {
        exe.to_string()
    } else {
        linux_entry(exe)
    }
}

/// A Windows path can't hold a quote, so quotes around it are enough.
fn windows_entry(exe: &str) -> String {
    format!("\"{exe}\"")
}

/// The Exec line of a desktop entry: `"`, `` ` ``, `$` and `\` are escaped inside the quotes and `%` is doubled. The
/// file's own strings then need every backslash doubled.
fn linux_entry(exe: &str) -> String {
    let mut quoted = String::from("\"");
    for c in exe.chars() {
        match c {
            '"' | '`' | '$' | '\\' => quoted.extend(['\\', c]),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

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
    fn only_a_released_build_opened_with_no_options_is_a_players_start() {
        assert!(players_start(true, "2026.11.22.163349"));
        assert!(!players_start(false, "2026.11.22.163349"));
        assert!(!players_start(true, "dev"));
        assert!(!players_start(false, "dev"));
    }

    #[test]
    fn quotes_the_path_in_the_startup_entry() {
        let installed = "C:\\Users\\me\\AppData\\Local\\Programs\\LFG+ Helper\\lfgplus-helper.exe";
        assert_eq!(windows_entry(installed), format!("\"{installed}\""));
        assert_eq!(linux_entry("/home/me/.local/bin/lfgplus-helper"), "\"/home/me/.local/bin/lfgplus-helper\"");
        assert_eq!(linux_entry("/home/me too/lfgplus-helper"), "\"/home/me too/lfgplus-helper\"");
        assert_eq!(linux_entry("/a$b`c\"d%e\\f"), r#""/a\\$b\\`c\\"d%%e\\\\f""#);
    }

    #[test]
    fn writes_the_entry_this_system_needs() {
        let entry = entry("/opt/lfgplus-helper");
        if cfg!(target_os = "macos") {
            assert_eq!(entry, "/opt/lfgplus-helper");
        } else {
            assert_eq!(entry, "\"/opt/lfgplus-helper\"");
        }
    }

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
