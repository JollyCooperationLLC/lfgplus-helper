//! The command line:
//!
//!   lfgplus-helper                     run in the tray
//!   lfgplus-helper --once              one pass, print where things stand, exit (no tray)
//!   lfgplus-helper --list              print the folders searched and the saved data found, exit
//!   lfgplus-helper --version           print the version, exit
//!   --api URL      where lfg.plus answers (default https://lfg.plus, or LFGPLUS_API)
//!   --wow PATH     the World of Warcraft folder to use
//!   --state FILE   the helper's state file (default: in the system's config folder)
//!   --interval S   seconds between looks at the saved data (default 60, at least 5)

use std::path::PathBuf;

use crate::state;

pub const SITE: &str = "https://lfg.plus";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub api: String,
    pub wow: Option<PathBuf>,
    pub state: PathBuf,
    pub interval: u64,
    pub once: bool,
    pub list: bool,
    pub version: bool,
    /// Started with no options at all, as a player starts it: only then does it install itself or
    /// change what starts with the computer.
    pub plain: bool,
}

/// Reads `args` (without the program's name). `api_env` is `LFGPLUS_API`, if set. An unknown option or a missing
/// value is an Err with the message to print.
pub fn parse(args: impl IntoIterator<Item = String>, api_env: Option<String>) -> Result<Options, String> {
    let mut args = args.into_iter().peekable();
    let plain = args.peek().is_none();
    let mut o = Options {
        api: api_env.unwrap_or_else(|| SITE.into()),
        wow: None,
        state: state::app_dir().join("state.txt"),
        interval: 60,
        once: false,
        list: false,
        version: false,
        plain,
    };
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--api" => o.api = value()?,
            "--wow" => o.wow = Some(PathBuf::from(value()?)),
            "--state" => o.state = PathBuf::from(value()?),
            "--interval" => o.interval = value()?.parse().map_err(|_| "--interval takes seconds".to_string())?,
            "--once" => o.once = true,
            "--list" => o.list = true,
            "--version" => o.version = true,
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    o.interval = o.interval.max(5);
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_plain_start_uses_the_defaults() {
        let o = parse(args(&[]), None).expect("parses");
        assert_eq!(o.api, SITE);
        assert_eq!((o.wow, o.interval, o.once, o.list, o.version, o.plain), (None, 60, false, false, false, true));
        assert!(o.state.ends_with("state.txt"));
        assert_eq!(
            parse(args(&[]), Some("http://localhost:8787".into())).expect("parses").api,
            "http://localhost:8787"
        );
    }

    #[test]
    fn reads_every_option() {
        let o = parse(
            args(&[
                "--api",
                "http://x",
                "--wow",
                "/games/wow",
                "--state",
                "/tmp/s.txt",
                "--interval",
                "1",
                "--once",
                "--list",
                "--version",
            ]),
            Some("http://ignored".into()),
        )
        .expect("parses");
        assert_eq!(o.api, "http://x");
        assert_eq!(o.wow, Some(PathBuf::from("/games/wow")));
        assert_eq!(o.state, PathBuf::from("/tmp/s.txt"));
        assert_eq!(o.interval, 5, "never looks more often than every 5 seconds");
        assert!(o.once && o.list && o.version && !o.plain);
    }

    #[test]
    fn says_what_is_wrong() {
        assert_eq!(parse(args(&["--wow"]), None), Err("--wow needs a value".into()));
        assert_eq!(parse(args(&["--interval", "soon"]), None), Err("--interval takes seconds".into()));
        assert_eq!(parse(args(&["--fast"]), None), Err("unknown option --fast".into()));
    }
}
