//! The update check against a small local server that answers like the releases page.

mod common;

use common::serve;
use lfgplus_helper::update;

#[test]
fn reads_the_latest_release_from_the_redirect() {
    let (site, rx) = serve(vec![(302, vec![("Location", "/o/r/releases/tag/v2026.11.21.101500".into())], "")]);
    let releases = format!("{site}/o/r/releases/");
    assert_eq!(update::latest(&releases, "LFGPlusHelper/test").as_deref(), Some("2026.11.21.101500"));
    let asked = rx.recv().expect("request");
    assert_eq!((asked.method.as_str(), asked.path.as_str()), ("GET", "/o/r/releases/latest"));
    assert_eq!(asked.header("User-Agent"), Some("LFGPlusHelper/test"));
    assert_eq!(asked.header("Cookie"), None);
}

#[test]
fn says_nothing_when_it_cant_tell() {
    for answer in [
        (404, Vec::new(), "not found"),
        (200, Vec::new(), "a page"),
        (302, Vec::new(), ""),
        (302, vec![("Location", "/o/r/releases".to_string())], ""),
    ] {
        let (site, _rx) = serve(vec![answer]);
        assert_eq!(update::latest(&format!("{site}/o/r/releases"), "x"), None);
    }
    assert_eq!(update::latest("http://127.0.0.1:9/releases", "x"), None, "nothing listening");
}
