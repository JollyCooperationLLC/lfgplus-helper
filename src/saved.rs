//! What the helper takes from the addon's saved data (`LFGPlusDB`): the upload lines and the numbers that say which of
//! them still wait. Everything else in the file, including the player's setup code, is dropped as soon as it is parsed
//! and never kept, logged or sent.

use crate::lua::{self, Table, Value};

/// The site refuses records older than 30 days; like the addon, lines older than that less an hour aren't sent.
pub const UPLOAD_MAX_AGE: u64 = 30 * 86_400 - 3_600;

/// Most lines one upload may carry, as the site takes them.
pub const MAX_LINES_PER_UPLOAD: usize = 500;

/// One upload line as the addon prints it, with the storage number and time the addon keeps beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub seq: u64,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Saved {
    /// The install's id, made once by the addon; acks are filed under it. None from addons that don't make one.
    pub install: Option<String>,
    /// The newest storage number given to a line.
    pub last_seq: u64,
    /// Lines numbered up to this were marked uploaded in the game.
    pub uploaded_seq: u64,
    /// Our own records, then the copies carried for others, in the addon's order.
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The file isn't the Lua the client writes (or was caught half written).
    Parse(lua::ParseError),
    /// The file has no `LFGPlusDB` table.
    NoData,
}

/// Reads the addon's saved data from the text of its SavedVariables file.
pub fn read(source: &str) -> Result<Saved, ReadError> {
    let globals = lua::parse(source).map_err(ReadError::Parse)?;
    let db = lua::global(&globals, "LFGPlusDB").and_then(Value::as_table).ok_or(ReadError::NoData)?;
    let mut lines = list_lines(db, "records");
    lines.extend(list_lines(db, "carried"));
    Ok(Saved {
        install: db.get("install").and_then(Value::as_str).filter(|s| valid_install(s)).map(str::to_owned),
        last_seq: db.get("lastSeq").and_then(Value::as_u64).unwrap_or(0),
        uploaded_seq: db.get("uploadedSeq").and_then(Value::as_u64).unwrap_or(0),
        lines,
    })
}

fn list_lines(db: &Table, name: &str) -> Vec<Line> {
    let Some(list) = db.get(name).and_then(Value::as_table) else { return Vec::new() };
    list.list()
        .into_iter()
        .filter_map(Value::as_table)
        .filter_map(|t| {
            Some(Line {
                text: t.get("text").and_then(Value::as_str)?.to_owned(),
                seq: t.get("seq").and_then(Value::as_u64).unwrap_or(0),
                at: t.get("at").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .collect()
}

/// An install id the helper will file acks under: letters and digits only, so it's safe in a file name or a Lua key.
pub fn valid_install(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// True for a line the site's request schema takes: 16 to 400 characters of the record alphabet. Anything else
/// would make the site refuse the whole upload, so it's never sent.
pub fn sendable_text(text: &str) -> bool {
    (16..=400).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'_' | b'.' | b':' | b'-'))
}

/// The lines still waiting at `now` (Unix seconds): numbered above `sent_up_to`, young enough for the site, and sendable.
pub fn waiting(saved: &Saved, sent_up_to: u64, now: u64) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    saved
        .lines
        .iter()
        .filter(|l| l.seq > sent_up_to && now.saturating_sub(l.at) <= UPLOAD_MAX_AGE && sendable_text(&l.text))
        .filter(|l| seen.insert(l.text.as_str()))
        .map(|l| l.text.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REC_A: &str = "LFGR1:AAAAAAAAAAAAAAAA";
    const REC_B: &str = "LFGR1:BBBBBBBBBBBBBBBB";
    const REC_C: &str = "LFGR1:CCCCCCCCCCCCCCCC";

    fn file(extra: &str) -> String {
        format!(
            "LFGPlusDB = {{\n[\"setupCode\"] = \"not-kept\",\n[\"install\"] = \"a1b2c3d4e5f60718\",\n[\"lastSeq\"] = 9,\n[\"uploadedSeq\"] = 2,\n\
             [\"records\"] = {{\n{{ [\"text\"] = \"{REC_A}\", [\"seq\"] = 2, [\"at\"] = 1000, [\"dir\"] = \"given\" }},\n\
             {{ [\"text\"] = \"{REC_B}\", [\"seq\"] = 5, [\"at\"] = 2000 }},\n{{ [\"seq\"] = 6 }},\n7,\n}},\n\
             [\"carried\"] = {{\n{{ [\"text\"] = \"{REC_C}\", [\"seq\"] = 9, [\"at\"] = 3000 }},\n}},\n{extra}}}\n"
        )
    }

    #[test]
    fn reads_the_lines_and_numbers() {
        let saved = read(&file("")).expect("reads");
        assert_eq!(saved.install.as_deref(), Some("a1b2c3d4e5f60718"));
        assert_eq!(saved.last_seq, 9);
        assert_eq!(saved.uploaded_seq, 2);
        let texts: Vec<_> = saved.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, [REC_A, REC_B, REC_C]);
        assert_eq!(saved.lines[1], Line { text: REC_B.into(), seq: 5, at: 2000 });
    }

    #[test]
    fn keeps_nothing_else_from_the_file() {
        let saved = read(&file("")).expect("reads");
        assert!(!format!("{saved:?}").contains("not-kept"));
    }

    #[test]
    fn missing_fields_read_as_empty() {
        let saved = read("LFGPlusDB = { [\"records\"] = 1, [\"install\"] = \"no!\" }").expect("reads");
        assert_eq!(saved, Saved::default());
        let saved = read("LFGPlusDB = { [\"records\"] = { { [\"text\"] = \"x\" } } }").expect("reads");
        assert_eq!(saved.lines, [Line { text: "x".into(), seq: 0, at: 0 }]);
    }

    #[test]
    fn refuses_files_without_data() {
        assert_eq!(read("LFGPlusLog = {}"), Err(ReadError::NoData));
        assert_eq!(read("LFGPlusDB = 1"), Err(ReadError::NoData));
        assert!(matches!(read("LFGPlusDB = {"), Err(ReadError::Parse(_))));
    }

    #[test]
    fn waiting_lines_skip_sent_old_and_unsendable_ones() {
        let saved = read(&file("")).expect("reads");
        assert_eq!(waiting(&saved, 2, 3000), [REC_B, REC_C]);
        assert_eq!(waiting(&saved, 5, 3000), [REC_C]);
        assert_eq!(waiting(&saved, 0, 1000 + UPLOAD_MAX_AGE), [REC_A, REC_B, REC_C]);
        assert_eq!(waiting(&saved, 0, 2001 + UPLOAD_MAX_AGE), [REC_C]);
        let mut odd = saved.clone();
        odd.lines.push(Line { text: "short".into(), seq: 10, at: 3000 });
        odd.lines.push(Line { text: format!("{REC_C}\"x"), seq: 11, at: 3000 });
        odd.lines.push(Line { text: REC_C.into(), seq: 12, at: 3000 });
        assert_eq!(waiting(&odd, 5, 3000), [REC_C]);
    }

    #[test]
    fn checks_ids_and_texts() {
        assert!(valid_install("abcdEF12"));
        assert!(!valid_install("abc"));
        assert!(!valid_install(&"a".repeat(65)));
        assert!(!valid_install("abcd-ef12"));
        assert!(sendable_text("LFGR1:abc+/=_.:-0123456789"));
        assert!(!sendable_text(&"a".repeat(401)));
        assert!(!sendable_text("LFGR1:abcdefghij k"));
    }
}
