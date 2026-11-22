//! One pass of the helper: find the saved data, send what waits, remember it, and tell the addon.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::acks;
use crate::saved::{self, MAX_LINES_PER_UPLOAD};
use crate::state::State;
use crate::upload::{UploadError, Uploader};
use crate::wow::{self, SavedFile};

/// First wait after a failed upload; it doubles up to `MAX_BACKOFF`.
pub const FIRST_BACKOFF: u64 = 60;
pub const MAX_BACKOFF: u64 = 30 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// No WoW folder found in the usual places.
    NoWow,
    /// No WoW in the folder the player chose.
    NotInFolder,
    /// WoW found, but no LFG+ saved data in it yet.
    NoData,
    /// Everything waiting was sent. The last delivery, if any: (Unix seconds, lines).
    Ready(Option<(u64, u64)>),
    /// The last upload failed; it's tried again at the given time.
    Problem(UploadError, u64),
    /// lfg.plus refused an upload, with this status. The other files went up; that one is tried again when the game
    /// writes it again, or when the player asks.
    Refused(u16),
}

/// A file's modification time and length: when either changes, the game wrote it again.
type Stamp = (Option<SystemTime>, u64);

pub struct Engine<U: Uploader> {
    uploader: U,
    pub state: State,
    state_path: PathBuf,
    seen: HashMap<PathBuf, Stamp>,
    retry: Option<(u64, UploadError)>,
    backoff: u64,
    /// Files whose last upload lfg.plus refused, and its status.
    refused: HashMap<PathBuf, u16>,
    /// Lines for the log file, drained by the caller. Counts and paths only: never a record or anything from the file.
    pub journal: Vec<String>,
}

impl<U: Uploader> Engine<U> {
    pub fn new(uploader: U, state_path: PathBuf) -> Self {
        let state = State::load(&state_path);
        Engine {
            uploader,
            state,
            state_path,
            seen: HashMap::new(),
            retry: None,
            backoff: FIRST_BACKOFF,
            refused: HashMap::new(),
            journal: Vec::new(),
        }
    }

    /// The folders searched: the one the player chose, else the usual places.
    pub fn roots(&self) -> Vec<PathBuf> {
        match &self.state.wow {
            Some(dir) => vec![dir.clone()],
            None => wow::usual_roots(),
        }
    }

    /// Uses another WoW folder from now on, and looks at everything in it again.
    pub fn choose_wow(&mut self, dir: PathBuf) {
        self.state.wow = Some(dir);
        self.start_over();
    }

    /// Goes back to searching the usual places, and looks at everything again.
    pub fn forget_wow(&mut self) {
        self.state.wow = None;
        self.start_over();
    }

    fn start_over(&mut self) {
        self.seen.clear();
        self.refused.clear();
        self.retry = None;
        self.save();
    }

    /// Remembers that the first-start window was shown.
    pub fn set_welcomed(&mut self) {
        self.state.welcomed = true;
        self.save();
    }

    pub fn set_autostart(&mut self, on: bool) {
        self.state.autostart = Some(on);
        self.save();
    }

    fn save(&mut self) {
        if let Err(e) = self.state.save(&self.state_path) {
            self.journal.push(format!("couldn't save the helper's state: {e}"));
        }
    }

    /// One pass at `now` (Unix seconds). Looks only at files the game wrote since the last pass, unless `force` (the
    /// player asked) or a failed upload is due again.
    pub fn tick(&mut self, now: u64, force: bool) -> Status {
        let roots = self.roots();
        let files = wow::saved_files(&roots);
        if files.is_empty() {
            return if roots.iter().any(|r| !wow::game_dirs(r).is_empty()) {
                Status::NoData
            } else if self.state.wow.is_some() {
                Status::NotInFolder
            } else {
                Status::NoWow
            };
        }
        let due = self.retry.as_ref().is_some_and(|(at, _)| now >= *at);
        if let Some((at, problem)) = &self.retry
            && !due
            && !force
        {
            return Status::Problem(problem.clone(), *at);
        }
        let mut sent = 0;
        for file in files {
            let stamp = stamp(&file);
            if !force && !due && self.seen.get(&file.path) == Some(&stamp) {
                continue;
            }
            match self.process(&file, now) {
                Ok(count) => {
                    sent += count;
                    self.refused.remove(&file.path);
                }
                // Something about this upload itself: sending it again won't help until it changes. The others go on.
                Err(UploadError::Refused(code)) => {
                    self.journal.push(format!("lfg.plus refused an upload from {} ({code})", file.path.display()));
                    self.refused.insert(file.path.clone(), code);
                }
                Err(problem) => {
                    let at = now + self.backoff;
                    self.journal.push(format!("upload failed ({problem:?}); trying again in {} s", self.backoff));
                    self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
                    self.retry = Some((at, problem.clone()));
                    return Status::Problem(problem, at);
                }
            }
            // A file that couldn't be read is marked seen too: it's read again when the game writes it again.
            self.seen.insert(file.path.clone(), stamp);
        }
        self.retry = None;
        self.backoff = FIRST_BACKOFF;
        if sent > 0 {
            self.state.last_upload = Some((now, sent));
            self.save();
        }
        match self.refused.values().next() {
            Some(code) => Status::Refused(*code),
            None => Status::Ready(self.state.last_upload),
        }
    }

    /// Sends one file's waiting lines and acks them. Returns how many lines were sent.
    fn process(&mut self, file: &SavedFile, now: u64) -> Result<u64, UploadError> {
        let place = file.path.display().to_string();
        let saved = match fs::read_to_string(&file.path)
            .map_err(|e| e.to_string())
            .and_then(|t| saved::read(&t).map_err(|e| format!("{e:?}")))
        {
            Ok(saved) => saved,
            Err(e) => {
                self.journal.push(format!("couldn't read {place}: {e}"));
                return Ok(0);
            }
        };
        let key = saved.install.clone().unwrap_or_else(|| format!("file:{place}"));
        let sent_up_to = saved.uploaded_seq.max(self.state.sent.get(&key).copied().unwrap_or(0));
        let lines = saved::waiting(&saved, sent_up_to, now);
        for part in lines.chunks(MAX_LINES_PER_UPLOAD) {
            self.uploader.upload(part)?;
        }
        if !lines.is_empty() {
            self.journal.push(format!("sent {} lines from {place}", lines.len()));
        }
        // Every line numbered up to the file's newest was sent now or before, or is too old for the site to take.
        let acked = sent_up_to.max(saved.last_seq);
        if self.state.sent.get(&key) != Some(&acked) {
            self.state.sent.insert(key, acked);
            self.save();
        }
        if let Some(install) = saved.install
            && acked > saved.uploaded_seq
        {
            match acks::write(&file.game_dir, &BTreeMap::from([(install, acked)])) {
                Ok(()) => {}
                Err(acks::WriteError::NoAddon) => {
                    self.journal.push(format!("LFG+ isn't installed in {}; no acks written", file.game_dir.display()))
                }
                Err(acks::WriteError::Io(e)) => {
                    self.journal.push(format!("couldn't write acks in {}: {e}", file.game_dir.display()))
                }
            }
        }
        Ok(lines.len() as u64)
    }
}

fn stamp(file: &SavedFile) -> Stamp {
    fs::metadata(&file.path).map(|m| (m.modified().ok(), m.len())).unwrap_or((None, 0))
}

/// The tray's one status line.
pub fn status_line(status: &Status, now: u64) -> String {
    match status {
        Status::NoWow => "Can't find World of Warcraft".into(),
        Status::NotInFolder => "No World of Warcraft in the folder you chose".into(),
        Status::NoData => "No LFG+ data found yet".into(),
        Status::Ready(None) => "Nothing sent yet".into(),
        Status::Ready(Some((at, count))) => {
            let what = if *count == 1 { "1 endorsement".to_string() } else { format!("{count} endorsements") };
            format!("Sent {what} {}", ago(now.saturating_sub(*at)))
        }
        Status::Problem(UploadError::Unreachable, _) => "Can't reach lfg.plus. Trying again soon.".into(),
        Status::Problem(UploadError::Busy, _) => "lfg.plus is busy. Trying again soon.".into(),
        Status::Problem(UploadError::Refused(code), _) | Status::Refused(code) => {
            format!("lfg.plus refused an upload ({code}). Trying again after you play.")
        }
    }
}

fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => "just now".into(),
        60..120 => "1 minute ago".into(),
        120..3_600 => format!("{} minutes ago", seconds / 60),
        3_600..7_200 => "1 hour ago".into(),
        7_200..86_400 => format!("{} hours ago", seconds / 3_600),
        86_400..172_800 => "yesterday".into(),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_where_things_stand() {
        let now = 1_000_000;
        assert_eq!(status_line(&Status::NoWow, now), "Can't find World of Warcraft");
        assert_eq!(status_line(&Status::NotInFolder, now), "No World of Warcraft in the folder you chose");
        assert_eq!(status_line(&Status::NoData, now), "No LFG+ data found yet");
        assert_eq!(status_line(&Status::Ready(None), now), "Nothing sent yet");
        assert_eq!(status_line(&Status::Ready(Some((now - 5, 1))), now), "Sent 1 endorsement just now");
        assert_eq!(status_line(&Status::Ready(Some((now - 300, 12))), now), "Sent 12 endorsements 5 minutes ago");
        assert_eq!(
            status_line(&Status::Problem(UploadError::Unreachable, 0), now),
            "Can't reach lfg.plus. Trying again soon."
        );
        assert_eq!(status_line(&Status::Problem(UploadError::Busy, 0), now), "lfg.plus is busy. Trying again soon.");
        assert_eq!(
            status_line(&Status::Problem(UploadError::Refused(400), 0), now),
            "lfg.plus refused an upload (400). Trying again after you play."
        );
        assert_eq!(
            status_line(&Status::Refused(413), now),
            "lfg.plus refused an upload (413). Trying again after you play."
        );
    }

    #[test]
    fn counts_time_in_plain_words() {
        assert_eq!(ago(90), "1 minute ago");
        assert_eq!(ago(3_700), "1 hour ago");
        assert_eq!(ago(3 * 3_600), "3 hours ago");
        assert_eq!(ago(90_000), "yesterday");
        assert_eq!(ago(3 * 86_400), "3 days ago");
    }
}
