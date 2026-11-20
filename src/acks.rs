//! Tells the addon which lines went up. The game rewrites its own saved data at every logout and `/reload`, so the
//! helper never writes there. It keeps a tiny add-on of its own beside LFG+, `LFGPlus_Helper`, which the game only
//! ever reads (at login and `/reload`) and which nothing but the helper writes:
//!
//! ```lua
//! LFGPlusHelperAcks = {
//!     ["a1b2c3d4e5f60718"] = 812,
//! }
//! ```
//!
//! Each entry is an install of LFG+ (one per game account) and the storage number every line up to which was
//! uploaded. LFG+ reads it at login and marks those lines uploaded, as its "I uploaded these" button does.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::lua;
use crate::saved::valid_install;

pub const ADDON_FOLDER: &str = "LFGPlus_Helper";
pub const ACKS_FILE: &str = "Acks.lua";

/// The add-on folder inside a game folder (`_retail_`, `_classic_`…).
pub fn addon_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("Interface").join("AddOns").join(ADDON_FOLDER)
}

fn lfgplus_toc(game_dir: &Path) -> PathBuf {
    game_dir.join("Interface").join("AddOns").join("LFGPlus").join("LFGPlus.toc")
}

/// The `## Interface:` line of a TOC, so ours matches LFG+'s and the game never marks it out of date.
pub fn interface_line(toc: &str) -> Option<String> {
    toc.lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .find(|l| l.starts_with("## Interface:"))
        .filter(|l| l.bytes().all(|b| b.is_ascii_digit() || b" ,:#Interface".contains(&b)))
        .map(str::to_owned)
}

pub fn toc_text(interface: &str) -> String {
    format!(
        "{interface}\n## Title: LFG|cff1eff00+|r Helper\n## Notes: Tells LFG+ which endorsements the LFG+ helper app uploaded. Written by the app; safe to delete.\n\
         ## Author: Jolly Cooperation\n## Dependencies: LFGPlus\n## X-Website: https://lfg.plus\n\n{ACKS_FILE}\n"
    )
}

pub fn acks_text(acks: &BTreeMap<String, u64>) -> String {
    let mut text = String::from(
        "-- Written by the LFG+ helper app. LFG+ reads it at login; edits are overwritten.\nLFGPlusHelperAcks = {\n",
    );
    for (install, seq) in acks.iter().filter(|(id, _)| valid_install(id)) {
        text.push_str(&format!("    [{}] = {seq},\n", lua::quote(install)));
    }
    text.push_str("}\n");
    text
}

/// The acks a game folder's file holds now, read back with the same parser; empty when there's none or it's unreadable.
pub fn read_acks(game_dir: &Path) -> BTreeMap<String, u64> {
    let Ok(text) = fs::read_to_string(addon_dir(game_dir).join(ACKS_FILE)) else { return BTreeMap::new() };
    let Ok(globals) = lua::parse(&text) else { return BTreeMap::new() };
    let Some(table) = lua::global(&globals, "LFGPlusHelperAcks").and_then(lua::Value::as_table) else {
        return BTreeMap::new();
    };
    table
        .entries
        .iter()
        .filter_map(|(k, v)| match k {
            lua::Key::Str(id) if valid_install(id) => Some((id.clone(), v.as_u64()?)),
            _ => None,
        })
        .collect()
}

#[derive(Debug)]
pub enum WriteError {
    /// LFG+ isn't installed in this game folder, so there's nothing to tell.
    NoAddon,
    Io(io::Error),
}

impl From<io::Error> for WriteError {
    fn from(e: io::Error) -> Self {
        WriteError::Io(e)
    }
}

/// Merges `acks` into the game folder's file (keeping the higher number per install) and writes the add-on where it
/// changed. Each file is written beside its place and renamed over it, so the game never reads half a file.
pub fn write(game_dir: &Path, acks: &BTreeMap<String, u64>) -> Result<(), WriteError> {
    let toc = fs::read_to_string(lfgplus_toc(game_dir)).map_err(|_| WriteError::NoAddon)?;
    let interface = interface_line(&toc).ok_or(WriteError::NoAddon)?;
    let dir = addon_dir(game_dir);
    fs::create_dir_all(&dir)?;
    let mut merged = read_acks(game_dir);
    for (id, seq) in acks {
        let entry = merged.entry(id.clone()).or_insert(0);
        *entry = (*entry).max(*seq);
    }
    replace_if_changed(&dir.join(format!("{ADDON_FOLDER}.toc")), &toc_text(&interface))?;
    replace_if_changed(&dir.join(ACKS_FILE), &acks_text(&merged))?;
    Ok(())
}

fn replace_if_changed(path: &Path, text: &str) -> io::Result<()> {
    if fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOC: &str = "\u{feff}## Interface: 120105, 16001, 11509, 50504\n## Title: LFG+\n";

    fn game_with_addon() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        let lfg = dir.path().join("Interface").join("AddOns").join("LFGPlus");
        fs::create_dir_all(&lfg).expect("mkdir");
        fs::write(lfg.join("LFGPlus.toc"), TOC).expect("toc");
        dir
    }

    fn acks(entries: &[(&str, u64)]) -> BTreeMap<String, u64> {
        entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn copies_the_interface_line() {
        assert_eq!(interface_line(TOC).as_deref(), Some("## Interface: 120105, 16001, 11509, 50504"));
        assert_eq!(interface_line("## Title: x"), None);
        assert_eq!(interface_line("## Interface: 1\nos.exit()"), Some("## Interface: 1".into()));
        assert_eq!(interface_line("## Interface: 1 -- x"), None);
    }

    #[test]
    fn writes_the_addon_and_merges_acks() {
        let game = game_with_addon();
        write(game.path(), &acks(&[("aaaaaaaa11111111", 5)])).expect("writes");
        let dir = addon_dir(game.path());
        let toc = fs::read_to_string(dir.join("LFGPlus_Helper.toc")).expect("toc");
        assert!(toc.starts_with("## Interface: 120105, 16001, 11509, 50504\n"));
        assert!(toc.contains("## Dependencies: LFGPlus\n"));
        assert!(toc.ends_with("\nAcks.lua\n"));
        assert_eq!(read_acks(game.path()), acks(&[("aaaaaaaa11111111", 5)]));

        // Another install joins; a lower number never replaces a higher one.
        write(game.path(), &acks(&[("bbbbbbbb22222222", 3), ("aaaaaaaa11111111", 4)])).expect("writes");
        assert_eq!(read_acks(game.path()), acks(&[("aaaaaaaa11111111", 5), ("bbbbbbbb22222222", 3)]));
        assert!(!dir.join("Acks.tmp").exists());
    }

    #[test]
    fn cleans_up_when_the_file_cant_be_replaced() {
        let game = game_with_addon();
        // A folder where the acks file goes: the rename over it fails, and the temporary file is removed.
        let blocked = addon_dir(game.path()).join(ACKS_FILE);
        fs::create_dir_all(blocked.join("inside")).expect("mkdir");
        assert!(matches!(write(game.path(), &acks(&[("aaaaaaaa11111111", 1)])), Err(WriteError::Io(_))));
        assert!(!addon_dir(game.path()).join("Acks.tmp").exists());
    }

    #[test]
    fn leaves_an_unchanged_file_alone() {
        let game = game_with_addon();
        write(game.path(), &acks(&[("aaaaaaaa11111111", 5)])).expect("writes");
        let path = addon_dir(game.path()).join(ACKS_FILE);
        let before = fs::metadata(&path).and_then(|m| m.modified()).expect("mtime");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(game.path(), &acks(&[("aaaaaaaa11111111", 2)])).expect("writes");
        assert_eq!(fs::metadata(&path).and_then(|m| m.modified()).expect("mtime"), before);
    }

    #[test]
    fn needs_lfgplus_installed() {
        let game = tempfile::tempdir().expect("temp dir");
        assert!(matches!(write(game.path(), &acks(&[])), Err(WriteError::NoAddon)));
        let lfg = game.path().join("Interface").join("AddOns").join("LFGPlus");
        fs::create_dir_all(&lfg).expect("mkdir");
        fs::write(lfg.join("LFGPlus.toc"), "## Title: x\n").expect("toc");
        assert!(matches!(write(game.path(), &acks(&[])), Err(WriteError::NoAddon)));
    }

    #[test]
    fn reports_a_folder_it_cant_write() {
        let game = game_with_addon();
        // A file where the add-on's folder should be.
        fs::write(addon_dir(game.path()), "x").expect("file");
        assert!(matches!(write(game.path(), &acks(&[])), Err(WriteError::Io(_))));
    }

    #[test]
    fn reads_only_valid_acks() {
        let game = game_with_addon();
        assert!(read_acks(game.path()).is_empty());
        let dir = addon_dir(game.path());
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join(ACKS_FILE), "LFGPlusHelperAcks = {").expect("write");
        assert!(read_acks(game.path()).is_empty());
        fs::write(dir.join(ACKS_FILE), "Other = {}").expect("write");
        assert!(read_acks(game.path()).is_empty());
        fs::write(
            dir.join(ACKS_FILE),
            "LFGPlusHelperAcks = { [\"bad id\"] = 1, [\"aaaaaaaa11111111\"] = -1, [\"bbbbbbbb22222222\"] = 7, 9 }",
        )
        .expect("write");
        assert_eq!(read_acks(game.path()), acks(&[("bbbbbbbb22222222", 7)]));
    }

    #[test]
    fn writes_only_valid_ids() {
        let text = acks_text(&acks(&[("bad id", 1), ("aaaaaaaa11111111", 2)]));
        assert!(!text.contains("bad id"));
        assert!(text.contains("[\"aaaaaaaa11111111\"] = 2,"));
    }
}
