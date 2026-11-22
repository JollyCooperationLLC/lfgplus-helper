//! Whether a newer helper has been released. Once a day the helper asks the releases page where its latest release
//! is: the page answers with a redirect to that release's tag, so nothing but the address is read. The helper never
//! updates itself: the menu offers the download, and opening it replaces the old copy.

use std::time::Duration;

/// The releases page players download from.
pub const RELEASES: &str = "https://github.com/JollyCooperationLLC/lfgplus-helper/releases";

/// How often to ask, in seconds.
pub const CHECK_EVERY: u64 = 24 * 60 * 60;

/// A release version, `year.month.day.hourminutesecond`, with or without a leading `v`. None for anything else, such
/// as a development build's `dev`.
pub fn parse(version: &str) -> Option<[u64; 4]> {
    let mut parts = version.strip_prefix('v').unwrap_or(version).split('.');
    let mut out = [0; 4];
    for slot in &mut out {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *slot = part.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// True when `candidate` is a release later than `current`. False when either isn't a release version.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse(candidate), parse(current)), (Some(a), Some(b)) if a > b)
}

/// The version in a redirect to a release's tag (`…/releases/tag/v2026.11.20.172244`).
pub fn version_in(location: &str) -> Option<String> {
    let tag = location.rsplit_once("/releases/tag/")?.1;
    parse(tag)?;
    Some(tag.strip_prefix('v').unwrap_or(tag).to_owned())
}

/// Asks `releases` (the releases page) for the latest version. None when it can't tell: no answer, no redirect, or a
/// redirect anywhere but a release.
pub fn latest(releases: &str, user_agent: &str) -> Option<String> {
    let http: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let response =
        http.get(format!("{}/latest", releases.trim_end_matches('/'))).header("User-Agent", user_agent).call().ok()?;
    if !response.status().is_redirection() {
        return None;
    }
    version_in(response.headers().get("location")?.to_str().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_release_versions_only() {
        assert_eq!(parse("2026.11.20.172244"), Some([2026, 11, 20, 172244]));
        assert_eq!(parse("v2026.11.20.172244"), Some([2026, 11, 20, 172244]));
        for bad in ["dev", "", "2026.11.20", "2026.11.20.1.2", "2026.11.x.1", "2026..20.1", "2026.11.20.+1"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn compares_by_time() {
        assert!(is_newer("2026.11.21.000000", "2026.11.20.235959"));
        assert!(is_newer("2026.12.1.0", "2026.11.30.0"), "numbers, not text");
        assert!(!is_newer("2026.11.20.172244", "2026.11.20.172244"));
        assert!(!is_newer("2026.11.19.0", "2026.11.20.0"));
        assert!(!is_newer("2026.11.21.0", "dev"), "a development build never asks to update");
        assert!(!is_newer("dev", "2026.11.20.0"));
    }

    #[test]
    fn finds_the_version_in_a_redirect() {
        assert_eq!(
            version_in("https://example.com/o/r/releases/tag/v2026.11.20.172244").as_deref(),
            Some("2026.11.20.172244")
        );
        assert_eq!(version_in("/o/r/releases/tag/2026.11.20.172244").as_deref(), Some("2026.11.20.172244"));
        assert_eq!(version_in("https://example.com/o/r/releases"), None);
        assert_eq!(version_in("https://example.com/o/r/releases/tag/nightly"), None);
    }
}
