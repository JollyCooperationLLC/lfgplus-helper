//! The LFG+ helper: uploads the endorsements the LFG+ addon holds to lfg.plus, and tells the addon what went up.
//! Everything here builds and tests without a desktop; the tray lives in `main.rs`.

pub mod acks;
pub mod engine;
pub mod icon;
pub mod lua;
pub mod saved;
pub mod startup;
pub mod state;
pub mod upload;
pub mod wow;
