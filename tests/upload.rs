//! The upload client against a small local server that answers like lfg.plus.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

use lfgplus_helper::upload::{HttpUploader, UploadError, Uploader};

#[derive(Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// A scripted answer: status, headers, body.
type Answer = (u16, Vec<(&'static str, String)>, &'static str);

/// Serves one scripted answer per request, in order, and reports each request it got.
fn serve(answers: Vec<Answer>) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for (status, headers, body) in answers {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request_line = String::new();
            reader.read_line(&mut request_line).expect("read");
            let mut parts = request_line.split_whitespace();
            let (method, path) = (parts.next().unwrap_or("").to_owned(), parts.next().unwrap_or("").to_owned());
            let mut seen_headers = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read");
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some((k, v)) = line.split_once(':') {
                    seen_headers.push((k.trim().to_owned(), v.trim().to_owned()));
                }
            }
            let length = seen_headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            let mut body_bytes = vec![0; length];
            reader.read_exact(&mut body_bytes).expect("body");
            let _ = tx.send(Seen {
                method,
                path,
                headers: seen_headers,
                body: String::from_utf8_lossy(&body_bytes).into_owned(),
            });
            let mut response =
                format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
            for (k, v) in headers {
                response.push_str(&format!("{k}: {v}\r\n"));
            }
            response.push_str("\r\n");
            response.push_str(body);
            let mut stream = stream;
            stream.write_all(response.as_bytes()).expect("write");
        }
    });
    (address, rx)
}

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
