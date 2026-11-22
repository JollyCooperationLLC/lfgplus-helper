//! The upload client against a small local server that answers like lfg.plus.

mod common;

use common::{Answer, serve};
use lfgplus_helper::upload::{HttpUploader, UploadError, Uploader};

const OK: Answer = (200, Vec::new(), "{\"received\":2,\"rejected\":0}");

fn lines() -> Vec<String> {
    vec!["LFGR1:AAAAAAAAAAAAAAAA".into(), "LFGR1:BBBBBBBBBBBBBBBB".into()]
}

#[test]
fn posts_to_the_upload_endpoint_as_a_program() {
    let (api, rx) = serve(vec![OK, OK]);
    let mut uploader = HttpUploader::new(&format!("{api}/"));
    uploader.upload(&lines()).expect("uploads");

    let post = rx.recv().expect("upload request");
    assert_eq!((post.method.as_str(), post.path.as_str()), ("POST", "/api/v1/uploads"));
    // No Origin and no cookies: that is how the site knows a program's upload from a browser's.
    assert_eq!(post.header("Origin"), None);
    assert_eq!(post.header("Cookie"), None);
    assert_eq!(post.header("X-CSRF-Token"), None);
    assert_eq!(post.header("Content-Type"), Some("application/json"));
    assert!(post.header("User-Agent").is_some_and(|ua| ua.starts_with("LFGPlusHelper/")));
    assert_eq!(post.body, "{\"records\":[\"LFGR1:AAAAAAAAAAAAAAAA\",\"LFGR1:BBBBBBBBBBBBBBBB\"]}");

    uploader.upload(&lines()).expect("uploads again");
    assert_eq!(rx.recv().expect("upload request").path, "/api/v1/uploads");
}

#[test]
fn reports_each_kind_of_failure() {
    for (status, error) in [
        (429, UploadError::Busy),
        (400, UploadError::Refused(400)),
        (403, UploadError::Refused(403)),
        (500, UploadError::Unreachable),
        (302, UploadError::Unreachable),
    ] {
        let (api, _rx) = serve(vec![(status, Vec::new(), "{}")]);
        assert_eq!(HttpUploader::new(&api).upload(&lines()), Err(error), "status {status}");
    }
    // Nothing listening at all.
    assert_eq!(HttpUploader::new("http://127.0.0.1:9").upload(&lines()), Err(UploadError::Unreachable));
}
