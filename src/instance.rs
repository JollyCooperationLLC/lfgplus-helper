//! One helper at a time, and the files it keeps beside its state: a lock, the running copy's version, and the log.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The log starts over (keeping one old one) past this size.
pub const LOG_LIMIT: u64 = 256 * 1024;

fn beside(state_file: &Path, name: &str) -> PathBuf {
    state_file.with_file_name(name)
}

/// Takes the helper's lock beside `state_file`. None when another helper holds it (or the folder can't be written).
/// The lock lasts as long as the returned file is open.
pub fn lock(state_file: &Path) -> Option<File> {
    let path = beside(state_file, "helper.lock");
    fs::create_dir_all(path.parent()?).ok()?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Notes which version holds the lock, so a second copy started later can tell whether it's newer.
pub fn note_running(state_file: &Path, version: &str) {
    let _ = fs::write(beside(state_file, "running.txt"), version);
}

/// The version the running helper noted, if any.
pub fn running_version(state_file: &Path) -> Option<String> {
    let text = fs::read_to_string(beside(state_file, "running.txt")).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The log file beside `state_file`.
pub fn log_path(state_file: &Path) -> PathBuf {
    beside(state_file, "helper.log")
}

/// Appends `lines` to the log, each after `now` (Unix seconds), and empties `lines`. Past `LOG_LIMIT` the log is renamed
/// `helper.old.log` and a new one starts.
pub fn log(state_file: &Path, lines: &mut Vec<String>, now: u64) {
    if lines.is_empty() {
        return;
    }
    let path = log_path(state_file);
    if fs::metadata(&path).is_ok_and(|m| m.len() > LOG_LIMIT) {
        let _ = fs::rename(&path, beside(state_file, "helper.old.log"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        for line in lines.iter() {
            let _ = writeln!(file, "{now} {line}");
        }
    }
    lines.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_helper_at_a_time() {
        let dir = tempfile::tempdir().expect("temp dir");
        let state = dir.path().join("sub").join("state.txt");
        let first = lock(&state).expect("the first takes the lock");
        assert!(lock(&state).is_none(), "a second can't");
        drop(first);
        assert!(lock(&state).is_some(), "free again once the first is gone");
        // A folder that can't be made: no lock, no panic.
        fs::write(dir.path().join("file"), "x").expect("file");
        assert!(lock(&dir.path().join("file").join("state.txt")).is_none());
    }

    #[test]
    fn remembers_the_running_version() {
        let dir = tempfile::tempdir().expect("temp dir");
        let state = dir.path().join("state.txt");
        assert_eq!(running_version(&state), None);
        note_running(&state, "2026.11.20.172244");
        assert_eq!(running_version(&state).as_deref(), Some("2026.11.20.172244"));
        note_running(&state, " \n");
        assert_eq!(running_version(&state), None);
    }

    #[test]
    fn logs_and_starts_over_when_big() {
        let dir = tempfile::tempdir().expect("temp dir");
        let state = dir.path().join("state.txt");
        let mut lines = Vec::new();
        log(&state, &mut lines, 1);
        assert!(!log_path(&state).exists(), "nothing to say, no file");

        lines.push("sent 2 lines".to_string());
        log(&state, &mut lines, 7);
        assert!(lines.is_empty());
        assert_eq!(fs::read_to_string(log_path(&state)).expect("log"), "7 sent 2 lines\n");

        fs::write(log_path(&state), vec![b'x'; LOG_LIMIT as usize + 1]).expect("big log");
        lines.push("again".to_string());
        log(&state, &mut lines, 8);
        assert_eq!(fs::read_to_string(log_path(&state)).expect("log"), "8 again\n");
        assert!(dir.path().join("helper.old.log").exists());
    }
}
