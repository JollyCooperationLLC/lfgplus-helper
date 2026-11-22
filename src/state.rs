//! What the helper remembers between runs, in one small text file in the system's config folder: the WoW folder the
//! player chose, whether it starts with the computer, whether it has said hello, how far each install's lines were
//! sent, and the last upload.
//! One `name<TAB>value…` line each, so it's easy to read and to repair by hand.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    /// The WoW folder the player chose, if any; otherwise the usual places are searched.
    pub wow: Option<PathBuf>,
    /// Start with the computer. None until the player (or the first run) decides.
    pub autostart: Option<bool>,
    /// The first-start window was shown.
    pub welcomed: bool,
    /// Per install (or per file, for addons without an install id): every line numbered up to this was delivered.
    pub sent: BTreeMap<String, u64>,
    /// The last delivery: when (Unix seconds) and how many lines.
    pub last_upload: Option<(u64, u64)>,
}

impl State {
    pub fn parse(text: &str) -> State {
        let mut state = State::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["wow", path] if !path.is_empty() => state.wow = Some(PathBuf::from(path)),
                ["autostart", on] => state.autostart = Some(*on == "1"),
                ["welcomed", on] => state.welcomed = *on == "1",
                ["sent", key, seq] => {
                    if let Ok(seq) = seq.parse() {
                        state.sent.insert((*key).to_owned(), seq);
                    }
                }
                ["last", at, count] => {
                    if let (Ok(at), Ok(count)) = (at.parse(), count.parse()) {
                        state.last_upload = Some((at, count));
                    }
                }
                _ => {}
            }
        }
        state
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        if let Some(wow) = &self.wow {
            out.push_str(&format!("wow\t{}\n", wow.display()));
        }
        if let Some(on) = self.autostart {
            out.push_str(&format!("autostart\t{}\n", u8::from(on)));
        }
        if self.welcomed {
            out.push_str("welcomed\t1\n");
        }
        for (key, seq) in &self.sent {
            out.push_str(&format!("sent\t{key}\t{seq}\n"));
        }
        if let Some((at, count)) = self.last_upload {
            out.push_str(&format!("last\t{at}\t{count}\n"));
        }
        out
    }

    pub fn load(path: &Path) -> State {
        fs::read_to_string(path).map(|t| State::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        path.parent().map(fs::create_dir_all).transpose()?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, self.text())?;
        fs::rename(&temp, path)
    }
}

/// The helper's own folder: `%APPDATA%\LFGPlusHelper`, `~/Library/Application Support/LFGPlusHelper`,
/// `~/.config/LFGPlusHelper`.
pub fn app_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("LFGPlusHelper")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut state = State {
            wow: Some(PathBuf::from("C:\\Games\\World of Warcraft")),
            autostart: Some(false),
            welcomed: true,
            ..State::default()
        };
        state.sent.insert("a1b2c3d4e5f60718".into(), 812);
        state.sent.insert("file:C:\\x\\LFGPlus.lua".into(), 3);
        state.last_upload = Some((1_795_000_000, 12));
        assert_eq!(State::parse(&state.text()), state);
        assert_eq!(State::parse(&State::default().text()), State::default());
    }

    #[test]
    fn ignores_lines_it_cant_read() {
        let state =
            State::parse("wow\t\nautostart\t1\nwelcomed\t0\nsent\tk\tx\nlast\t1\nlast\tx\t1\njunk\nsent\tk\t4\n");
        assert_eq!(state, State { autostart: Some(true), sent: [("k".to_string(), 4)].into(), ..State::default() });
    }

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sub").join("state.txt");
        assert_eq!(State::load(&path), State::default());
        let state = State { autostart: Some(true), ..State::default() };
        state.save(&path).expect("saves");
        assert_eq!(State::load(&path), state);
        assert!(app_dir().ends_with("LFGPlusHelper"));
    }

    #[test]
    fn reports_a_folder_it_cant_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("file"), "x").expect("file");
        assert!(State::default().save(&dir.path().join("file").join("state.txt")).is_err());
    }
}
