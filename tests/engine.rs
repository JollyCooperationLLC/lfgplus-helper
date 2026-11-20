//! The whole pass on a fake WoW folder: what is sent, what is remembered, what the addon is told.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lfgplus_helper::acks;
use lfgplus_helper::engine::{Engine, FIRST_BACKOFF, MAX_BACKOFF, Status};
use lfgplus_helper::saved::UPLOAD_MAX_AGE;
use lfgplus_helper::state::State;
use lfgplus_helper::upload::{UploadError, Uploader};

const NOW: u64 = 1_800_000_000;
const INSTALL: &str = "a1b2c3d4e5f60718";

#[derive(Clone, Default)]
struct Fake {
    sent: Arc<Mutex<Vec<Vec<String>>>>,
    fail: Arc<Mutex<Option<UploadError>>>,
}

impl Uploader for Fake {
    fn upload(&mut self, lines: &[String]) -> Result<(), UploadError> {
        if let Some(e) = self.fail.lock().expect("lock").clone() {
            return Err(e);
        }
        self.sent.lock().expect("lock").push(lines.to_vec());
        Ok(())
    }
}

impl Fake {
    fn uploads(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut *self.sent.lock().expect("lock"))
    }
    fn fail_with(&self, e: Option<UploadError>) {
        *self.fail.lock().expect("lock") = e;
    }
}

fn record(n: u64) -> String {
    format!("LFGR1:{n:0>20}")
}

struct World {
    _dir: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
}

impl World {
    fn new() -> World {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("World of Warcraft");
        let lfg = root.join("_retail_/Interface/AddOns/LFGPlus");
        fs::create_dir_all(&lfg).expect("mkdir");
        fs::write(lfg.join("LFGPlus.toc"), "## Interface: 120105\n").expect("toc");
        let state = dir.path().join("helper/state.txt");
        World { _dir: dir, root, state }
    }

    fn saved_path(&self, account: &str) -> PathBuf {
        self.root.join("_retail_/WTF/Account").join(account).join("SavedVariables/LFGPlus.lua")
    }

    /// Writes an account's saved data as the client would: `records` (seq, at), `carried` (seq, at).
    fn write(
        &self,
        account: &str,
        install: Option<&str>,
        uploaded: Option<u64>,
        records: &[(u64, u64)],
        carried: &[(u64, u64)],
    ) {
        let entry = |(seq, at): &(u64, u64)| {
            format!("{{\n[\"text\"] = \"{}\",\n[\"seq\"] = {seq},\n[\"at\"] = {at},\n}}, -- [x]\n", record(*seq))
        };
        let last = records.iter().chain(carried).map(|(s, _)| *s).max().unwrap_or(0);
        let mut text = String::from("\nLFGPlusDB = {\n[\"setupCode\"] = \"never-sent\",\n");
        if let Some(id) = install {
            text.push_str(&format!("[\"install\"] = \"{id}\",\n"));
        }
        if let Some(u) = uploaded {
            text.push_str(&format!("[\"uploadedSeq\"] = {u},\n"));
        }
        text.push_str(&format!("[\"lastSeq\"] = {last},\n[\"records\"] = {{\n"));
        records.iter().for_each(|r| text.push_str(&entry(r)));
        text.push_str("},\n[\"carried\"] = {\n");
        carried.iter().for_each(|r| text.push_str(&entry(r)));
        text.push_str("},\n}\nLFGPlusLog = {\n}\n");
        let path = self.saved_path(account);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }

    fn engine(&self, fake: &Fake) -> Engine<Fake> {
        let mut engine = Engine::new(fake.clone(), self.state.clone());
        engine.choose_wow(self.root.clone());
        engine
    }

    fn acks(&self) -> std::collections::BTreeMap<String, u64> {
        acks::read_acks(&self.root.join("_retail_"))
    }
}

fn texts(seqs: &[u64]) -> Vec<String> {
    seqs.iter().map(|s| record(*s)).collect()
}

#[test]
fn sends_what_waits_once_and_acks_it() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), Some(1), &[(1, NOW), (2, NOW), (3, NOW - UPLOAD_MAX_AGE - 1)], &[(4, NOW)]);
    let fake = Fake::default();
    let mut engine = world.engine(&fake);

    assert_eq!(engine.tick(NOW, false), Status::Ready(Some((NOW, 2))));
    assert_eq!(fake.uploads(), [texts(&[2, 4])]);
    assert_eq!(world.acks().get(INSTALL), Some(&4));
    let toc =
        fs::read_to_string(acks::addon_dir(&world.root.join("_retail_")).join("LFGPlus_Helper.toc")).expect("toc");
    assert!(toc.starts_with("## Interface: 120105\n"));

    // Nothing changed: nothing read, nothing sent.
    assert_eq!(engine.tick(NOW + 60, false), Status::Ready(Some((NOW, 2))));
    assert!(fake.uploads().is_empty());
    // Asked again: read again, but those lines were sent already.
    engine.tick(NOW + 60, true);
    assert!(fake.uploads().is_empty());

    // The game wrote again with one more record, before it read the ack: only the new one goes.
    world.write("ONE", Some(INSTALL), Some(1), &[(1, NOW), (2, NOW), (5, NOW + 100)], &[(4, NOW)]);
    engine.tick(NOW + 120, false);
    assert_eq!(fake.uploads(), [texts(&[5])]);
    assert_eq!(world.acks().get(INSTALL), Some(&5));

    // A new helper (or a restart) remembers what was sent.
    let mut again = world.engine(&fake);
    again.tick(NOW + 180, true);
    assert!(fake.uploads().is_empty());
    assert_eq!(State::load(&world.state).sent.get(INSTALL), Some(&5));
}

#[test]
fn never_sends_the_setup_code_or_anything_else() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    let fake = Fake::default();
    world.engine(&fake).tick(NOW, false);
    let sent = fake.uploads().concat().join("\n");
    assert!(!sent.contains("never-sent"));
    assert!(!fs::read_to_string(&world.state).expect("state").contains("never-sent"));
}

#[test]
fn splits_big_uploads_into_parts_of_500() {
    let world = World::new();
    let records: Vec<(u64, u64)> = (1..=1001).map(|s| (s, NOW)).collect();
    world.write("ONE", Some(INSTALL), None, &records, &[]);
    let fake = Fake::default();
    assert_eq!(world.engine(&fake).tick(NOW, false), Status::Ready(Some((NOW, 1001))));
    let sizes: Vec<usize> = fake.uploads().iter().map(Vec::len).collect();
    assert_eq!(sizes, [500, 500, 1]);
}

#[test]
fn backs_off_after_a_failure_and_acks_nothing() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    let fake = Fake::default();
    fake.fail_with(Some(UploadError::Unreachable));
    let mut engine = world.engine(&fake);

    assert_eq!(engine.tick(NOW, false), Status::Problem(UploadError::Unreachable, NOW + FIRST_BACKOFF));
    assert!(world.acks().is_empty());
    // Not due yet: no try.
    fake.fail_with(None);
    assert_eq!(engine.tick(NOW + 10, false), Status::Problem(UploadError::Unreachable, NOW + FIRST_BACKOFF));
    assert!(fake.uploads().is_empty());
    // Due: tried again, and it goes.
    assert_eq!(engine.tick(NOW + FIRST_BACKOFF, false), Status::Ready(Some((NOW + FIRST_BACKOFF, 1))));
    assert_eq!(fake.uploads(), [texts(&[1])]);
    assert_eq!(world.acks().get(INSTALL), Some(&1));
}

#[test]
fn waits_longer_after_each_failure() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    let fake = Fake::default();
    fake.fail_with(Some(UploadError::Busy));
    let mut engine = world.engine(&fake);
    let mut now = NOW;
    let mut waits = Vec::new();
    for _ in 0..7 {
        match engine.tick(now, true) {
            Status::Problem(UploadError::Busy, at) => {
                waits.push(at - now);
                now = at;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(waits, [60, 120, 240, 480, 960, MAX_BACKOFF, MAX_BACKOFF]);
}

#[test]
fn handles_every_account_and_old_addons() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    world.write("TWO", Some("ffffffff00000000"), None, &[(1, NOW), (2, NOW)], &[]);
    // An addon from before install ids: sent and remembered by file, but no ack can name it.
    world.write("OLD", None, None, &[(7, NOW)], &[]);
    let fake = Fake::default();
    let mut engine = world.engine(&fake);
    assert_eq!(engine.tick(NOW, false), Status::Ready(Some((NOW, 4))));
    assert_eq!(fake.uploads().len(), 3);
    let acks = world.acks();
    assert_eq!(acks.get(INSTALL), Some(&1));
    assert_eq!(acks.get("ffffffff00000000"), Some(&2));
    assert_eq!(acks.len(), 2);
    let old: Vec<_> = engine.state.sent.iter().filter(|(k, _)| k.starts_with("file:") && k.contains("OLD")).collect();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].1, &7);
}

#[test]
fn skips_lines_marked_uploaded_in_the_game() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), Some(3), &[(1, NOW), (2, NOW), (3, NOW)], &[]);
    let fake = Fake::default();
    assert_eq!(world.engine(&fake).tick(NOW, false), Status::Ready(None));
    assert!(fake.uploads().is_empty());
    // Nothing went up, so the addon has nothing to learn.
    assert!(!acks::addon_dir(&world.root.join("_retail_")).exists());
}

#[test]
fn reads_a_half_written_file_again_once_the_game_finishes() {
    let world = World::new();
    let path = world.saved_path("ONE");
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(&path, "LFGPlusDB = {\n[\"records\"] = {").expect("write");
    let fake = Fake::default();
    let mut engine = world.engine(&fake);
    assert_eq!(engine.tick(NOW, false), Status::Ready(None));
    assert!(engine.journal.iter().any(|l| l.starts_with("couldn't read")));
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    engine.tick(NOW + 60, false);
    assert_eq!(fake.uploads(), [texts(&[1])]);
}

#[test]
fn says_when_lfgplus_isnt_installed() {
    let world = World::new();
    fs::remove_dir_all(world.root.join("_retail_/Interface")).expect("rm");
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    let fake = Fake::default();
    let mut engine = world.engine(&fake);
    engine.tick(NOW, false);
    assert_eq!(fake.uploads(), [texts(&[1])]);
    assert!(engine.journal.iter().any(|l| l.contains("no acks written")));
}

#[test]
fn says_when_acks_cant_be_written() {
    let world = World::new();
    world.write("ONE", Some(INSTALL), None, &[(1, NOW)], &[]);
    fs::write(acks::addon_dir(&world.root.join("_retail_")), "a file in the way").expect("file");
    let fake = Fake::default();
    let mut engine = world.engine(&fake);
    engine.tick(NOW, false);
    assert!(engine.journal.iter().any(|l| l.starts_with("couldn't write acks")));
}

#[test]
fn says_when_state_cant_be_saved() {
    let world = World::new();
    fs::create_dir_all(world.state.parent().expect("parent")).expect("mkdir");
    fs::create_dir_all(&world.state).expect("a folder where the state file goes");
    let fake = Fake::default();
    let mut engine = world.engine(&fake);
    assert!(engine.journal.iter().any(|l| l.starts_with("couldn't save")));
    engine.set_autostart(true);
    assert_eq!(engine.state.autostart, Some(true));
}

#[test]
fn says_what_it_cant_find() {
    let dir = tempfile::tempdir().expect("temp dir");
    let fake = Fake::default();
    let mut engine = Engine::new(fake.clone(), dir.path().join("state.txt"));
    engine.choose_wow(dir.path().join("nowhere"));
    assert_eq!(engine.tick(NOW, false), Status::NoWow);
    assert_eq!(engine.roots(), [dir.path().join("nowhere")]);
    let game = dir.path().join("wow").join("_retail_").join("WTF");
    fs::create_dir_all(&game).expect("mkdir");
    engine.choose_wow(dir.path().join("wow"));
    assert_eq!(engine.tick(NOW, false), Status::NoData);
}

#[test]
fn searches_the_usual_places_until_a_folder_is_chosen() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = Engine::new(Fake::default(), dir.path().join("state.txt"));
    assert_eq!(engine.roots(), lfgplus_helper::wow::usual_roots());
}
