//! Finds World of Warcraft and the LFG+ saved data in it. An install folder holds one folder per game
//! (`_retail_`, `_classic_`, `_classic_era_`…), each with `WTF/Account/<ACCOUNT>/SavedVariables/LFGPlus.lua`.

use std::fs;
use std::path::{Path, PathBuf};

pub const SAVED_FILE: &str = "LFGPlus.lua";

/// One LFG+ SavedVariables file and the game folder it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SavedFile {
    pub game_dir: PathBuf,
    pub path: PathBuf,
}

/// True for a folder that is one game: it has a `WTF` or `Interface` folder.
fn is_game_dir(dir: &Path) -> bool {
    dir.join("WTF").is_dir() || dir.join("Interface").is_dir()
}

/// The game folders in `root`: `root` itself when it is one, else its `_name_` folders that are.
pub fn game_dirs(root: &Path) -> Vec<PathBuf> {
    if is_game_dir(root) {
        return vec![root.to_path_buf()];
    }
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() > 2 && n.starts_with('_') && n.ends_with('_'))
                && is_game_dir(p)
        })
        .collect();
    dirs.sort();
    dirs
}

/// Every LFG+ SavedVariables file under the game folders of `roots`.
pub fn saved_files(roots: &[PathBuf]) -> Vec<SavedFile> {
    let mut files = Vec::new();
    for game_dir in roots.iter().flat_map(|r| game_dirs(r)) {
        let Ok(accounts) = fs::read_dir(game_dir.join("WTF").join("Account")) else { continue };
        for account in accounts.filter_map(Result::ok) {
            let path = account.path().join("SavedVariables").join(SAVED_FILE);
            if path.is_file() {
                files.push(SavedFile { game_dir: game_dir.clone(), path });
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Where World of Warcraft usually is on this computer, most likely first. Only folders that exist.
pub fn usual_roots() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = battlenet_roots();
    #[cfg(windows)]
    {
        candidates.extend(registry_root());
        for drive in b'C'..=b'Z' {
            let d = format!("{}:\\", drive as char);
            candidates.push(PathBuf::from(format!("{d}Program Files (x86)\\World of Warcraft")));
            candidates.push(PathBuf::from(format!("{d}Program Files\\World of Warcraft")));
            candidates.push(PathBuf::from(format!("{d}World of Warcraft")));
            candidates.push(PathBuf::from(format!("{d}Games\\World of Warcraft")));
        }
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from("/Applications/World of Warcraft"));
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("Applications/World of Warcraft"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = dirs::home_dir() {
        // Wine and its launchers keep Windows' folders inside a prefix.
        for prefix in [".wine", "Games/battlenet", "Games/world-of-warcraft", ".local/share/lutris/runners/wine"] {
            candidates.push(home.join(prefix).join("drive_c/Program Files (x86)/World of Warcraft"));
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    for c in candidates {
        if c.is_dir() && !roots.contains(&c) {
            roots.push(c);
        }
    }
    roots
}

/// Where the Battle.net app says it installed World of Warcraft: the install paths in its product list (`product.db`),
/// then its default install folder. Only those two values are taken from its files; nothing else in them is read.
fn battlenet_roots() -> Vec<PathBuf> {
    // The Battle.net app runs on Windows and macOS only; on Linux it lives inside Wine, searched in `usual_roots`.
    let has_app = cfg!(any(windows, target_os = "macos"));
    let products = if cfg!(windows) {
        "C:\\ProgramData\\Battle.net\\Agent\\product.db"
    } else {
        "/Users/Shared/Battle.net/Agent/product.db"
    };
    let config = dirs::config_dir().map(|d| d.join("Battle.net").join("Battle.net.config")).filter(|_| has_app);
    let mut roots: Vec<PathBuf> =
        if has_app { fs::read(products).map(|b| product_paths(&b)).unwrap_or_default() } else { Vec::new() };
    roots.extend(config.and_then(|f| fs::read_to_string(f).ok()).and_then(|t| default_install_path(&t)));
    roots
}

/// The World of Warcraft install paths in the Battle.net app's product list (a binary file): each readable run of text that
/// names the game, from its drive (`E:\`, `E:/`) or root (`/`), without a trailing game folder.
pub fn product_paths(bytes: &[u8]) -> Vec<PathBuf> {
    const NAME: &str = "World of Warcraft";
    bytes
        .split(|b| !(0x20..0x7f).contains(b))
        .filter_map(|run| std::str::from_utf8(run).ok())
        .filter(|run| run.contains(NAME))
        .filter_map(|run| {
            let drive = run
                .as_bytes()
                .windows(3)
                .position(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && matches!(w[2], b'/' | b'\\'));
            let start = drive.or_else(|| run.find('/'))?;
            let end = run.rfind(NAME)? + NAME.len();
            let path = run.get(start..)?;
            Some(install_root(Path::new(path.get(..end - start)?.trim_end_matches(['/', '\\']))))
        })
        .collect()
}

/// The Battle.net app's default install folder from its settings (JSON), plus the game's own folder name.
pub fn default_install_path(config: &str) -> Option<PathBuf> {
    let after = &config[config.find("\"DefaultInstallPath\"")? + "\"DefaultInstallPath\"".len()..];
    let value = after.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut path = String::new();
    let mut chars = value.chars();
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => path.push(match chars.next()? {
                '/' => '/',
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            }),
            c => path.push(c),
        }
    }
    (!path.is_empty()).then(|| Path::new(&path).join("World of Warcraft"))
}

/// The install folder the Battle.net app recorded, without its game folder (`…\World of Warcraft\_retail_\`).
#[cfg(windows)]
fn registry_root() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\WOW6432Node\\Blizzard Entertainment\\World of Warcraft")
        .and_then(|key| key.get_value::<String, _>("InstallPath"))
        .map(|path| install_root(Path::new(&path)))
        .ok()
}

/// A recorded install path without its trailing game folder, if it has one.
pub fn install_root(path: &Path) -> PathBuf {
    let is_game = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.len() > 2 && n.starts_with('_') && n.ends_with('_'));
    match (is_game, path.parent()) {
        (true, Some(parent)) => parent.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, "LFGPlusDB = {}").expect("write");
    }

    #[test]
    fn finds_every_account_in_every_game() {
        let root = tempfile::tempdir().expect("temp dir");
        let r = root.path();
        touch(&r.join("_retail_/WTF/Account/ONE/SavedVariables/LFGPlus.lua"));
        touch(&r.join("_retail_/WTF/Account/TWO/SavedVariables/LFGPlus.lua"));
        touch(&r.join("_retail_/WTF/Account/THREE/SavedVariables/Other.lua"));
        touch(&r.join("_classic_era_/WTF/Account/ONE/SavedVariables/LFGPlus.lua"));
        fs::create_dir_all(r.join("_empty_")).expect("mkdir");
        fs::create_dir_all(r.join("_forever_/Interface")).expect("mkdir");
        touch(&r.join("Data/WTF/Account/X/SavedVariables/LFGPlus.lua"));
        fs::write(r.join("_file_"), "x").expect("file");

        assert_eq!(game_dirs(r), [r.join("_classic_era_"), r.join("_forever_"), r.join("_retail_")]);
        let files = saved_files(&[r.to_path_buf(), r.join("_retail_")]);
        let paths: Vec<_> = files.iter().map(|f| f.path.strip_prefix(r).expect("inside").to_path_buf()).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("_classic_era_/WTF/Account/ONE/SavedVariables/LFGPlus.lua"),
                PathBuf::from("_retail_/WTF/Account/ONE/SavedVariables/LFGPlus.lua"),
                PathBuf::from("_retail_/WTF/Account/TWO/SavedVariables/LFGPlus.lua"),
            ]
        );
        assert_eq!(files[1].game_dir, r.join("_retail_"));
    }

    #[test]
    fn a_game_folder_is_its_own_root() {
        let root = tempfile::tempdir().expect("temp dir");
        touch(&root.path().join("WTF/Account/ONE/SavedVariables/LFGPlus.lua"));
        assert_eq!(game_dirs(root.path()), [root.path().to_path_buf()]);
        assert!(game_dirs(&root.path().join("missing")).is_empty());
    }

    #[test]
    fn strips_the_game_folder_from_an_install_path() {
        assert_eq!(install_root(Path::new("C:/WoW/_retail_")), PathBuf::from("C:/WoW"));
        assert_eq!(install_root(Path::new("C:/WoW")), PathBuf::from("C:/WoW"));
        assert_eq!(install_root(Path::new("_x_")), PathBuf::from(""));
    }

    #[test]
    fn reads_install_paths_from_the_product_list() {
        let mut db = vec![0x0a, 0x24];
        db.extend_from_slice(b"$E:/battlenet/WOW/World of Warcraft");
        db.extend_from_slice(&[0x12, 0x05]);
        db.extend_from_slice(b"wow_forever");
        db.extend_from_slice(&[0x1a, 0x30]);
        db.extend_from_slice(b"0C:\\Games\\World of Warcraft\\_retail_\\");
        db.push(0);
        db.extend_from_slice(b"/Applications/World of Warcraft");
        db.push(0);
        db.extend_from_slice(b"World of Warcraft has no path here");
        db.push(0xff);
        db.extend_from_slice(b"\xc3World of Warcraft");
        assert_eq!(
            product_paths(&db),
            [
                PathBuf::from("E:/battlenet/WOW/World of Warcraft"),
                PathBuf::from("C:\\Games\\World of Warcraft"),
                PathBuf::from("/Applications/World of Warcraft"),
            ]
        );
        assert!(product_paths(b"").is_empty());
    }

    #[test]
    fn reads_the_default_install_folder_from_the_settings() {
        let config = r#"{ "Client": { "Install": { "DefaultInstallPath": "E:\/battlenet\/WOW", "Other": "x" } } }"#;
        assert_eq!(default_install_path(config), Some(PathBuf::from("E:/battlenet/WOW").join("World of Warcraft")));
        assert_eq!(
            default_install_path(r#"{"DefaultInstallPath":"C:\\Games \"x\""}"#),
            Some(PathBuf::from("C:\\Games \"x\"").join("World of Warcraft"))
        );
        for bad in [
            r#"{}"#,
            r#"{"DefaultInstallPath" 1}"#,
            r#"{"DefaultInstallPath": 1}"#,
            r#"{"DefaultInstallPath": ""}"#,
            r#"{"DefaultInstallPath": "a\u0041"}"#,
            r#"{"DefaultInstallPath": "a"#,
            r#"{"DefaultInstallPath": "a\"#,
        ] {
            assert_eq!(default_install_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn usual_roots_only_lists_folders_that_exist() {
        assert!(usual_roots().iter().all(|p| p.is_dir()));
    }
}
