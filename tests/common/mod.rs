//! A small local server that answers like lfg.plus or the releases page, for the HTTP tests.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

#[derive(Debug)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// A scripted answer: status, headers, body.
pub type Answer = (u16, Vec<(&'static str, String)>, &'static str);

/// Serves one scripted answer per request, in order, and reports each request it got.
pub fn serve(answers: Vec<Answer>) -> (String, mpsc::Receiver<Seen>) {
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
