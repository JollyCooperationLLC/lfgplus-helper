//! The LFG+ helper: uploads the endorsements the LFG+ addon holds to lfg.plus, and tells the addon what went up.
//! Everything here builds and tests without a desktop; the tray lives in `main.rs`.

pub mod acks;
pub mod cli;
pub mod engine;
pub mod icon;
pub mod install;
pub mod instance;
pub mod lua;
pub mod notice;
pub mod saved;
pub mod startup;
pub mod state;
pub mod update;
pub mod upload;
pub mod wow;

/// This build's version: the release's `year.month.day.hourminutesecond`, or `dev` for a build made anywhere else
/// (build.rs).
pub const VERSION: &str = env!("LFGPLUS_HELPER_VERSION");

/// How the helper names itself to lfg.plus and the releases page.
pub fn user_agent() -> String {
    format!("LFGPlusHelper/{VERSION}")
}
