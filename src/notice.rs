//! What the helper says in a window, which it does only when the player has just opened it and can't see it yet: on
//! the first start, and when it's opened again while already running. Otherwise it only ever speaks through its menu.

use crate::update;

/// Where the icon is, on this system.
#[cfg(windows)]
pub const WHERE_IT_LIVES: &str = "Its icon is in the notification area, at the right end of the taskbar. If you don't \
                                  see it, click the ^ arrow there.";
#[cfg(target_os = "macos")]
pub const WHERE_IT_LIVES: &str = "Its icon is in the menu bar, at the top of the screen.";
#[cfg(all(unix, not(target_os = "macos")))]
pub const WHERE_IT_LIVES: &str =
    "Its icon is in the system tray. On GNOME, tray icons need the AppIndicator extension.";

const WHAT_IT_DOES: &str =
    "Whenever you log out of World of Warcraft or type /reload, it uploads your endorsements to lfg.plus.";

/// What happens next, once it runs from where it belongs.
#[cfg(target_os = "macos")]
const SETTLED: &str = "It starts with your Mac. You can eject the disk image and delete it.";
#[cfg(not(target_os = "macos"))]
const SETTLED: &str = "It starts with your computer, and you can delete the file you downloaded.";

/// A Mac app still running from the disk image or Downloads.
const MOVE_IT: &str = "Drag it to your Applications folder and open it from there, so it can start with your Mac.";

/// The first start. `must_move`: a Mac app still running from the disk image or Downloads (never true elsewhere).
pub fn welcome(must_move: bool) -> String {
    let after = if must_move { MOVE_IT } else { SETTLED };
    format!("LFG+ Helper is running. {WHERE_IT_LIVES} {WHAT_IT_DOES} {after}")
}

/// Opened again while a helper runs. `running` is the running copy's version, `mine` this one's.
pub fn already_running(running: Option<&str>, mine: &str) -> String {
    let mut text = format!("LFG+ Helper is already running. {WHERE_IT_LIVES}");
    if running.is_some_and(|running| update::is_newer(mine, running)) {
        text.push_str(&format!(" To update it to {mine}, choose Quit in its menu, then open this file again."));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcomes_and_says_where_to_look() {
        let text = welcome(false);
        assert!(text.starts_with("LFG+ Helper is running. Its icon is in the "));
        assert!(text.contains("/reload"));
        assert!(text.ends_with(SETTLED));
        assert!(welcome(true).ends_with("so it can start with your Mac."));
    }

    #[test]
    fn offers_the_update_only_when_this_copy_is_newer() {
        let plain = already_running(Some("2026.11.20.0"), "2026.11.20.0");
        assert!(plain.starts_with("LFG+ Helper is already running. Its icon is in the "));
        assert!(!plain.contains("update"));
        assert!(already_running(Some("2026.11.20.0"), "2026.11.21.0").contains("update it to 2026.11.21.0"));
        assert!(!already_running(Some("2026.11.21.0"), "2026.11.20.0").contains("update"));
        assert!(!already_running(None, "2026.11.21.0").contains("update"));
    }
}
