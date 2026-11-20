//! Sends upload lines to lfg.plus: `POST /api/v1/uploads` with `{ "records": [...] }`, the endpoint its upload page
//! uses. No login is needed: anyone may upload any record they hold. The helper sends no Origin and no cookies, which
//! is how the site tells a program's upload from a browser's and lets it past its cross-site request check.
//!
//! The answer says only how many lines arrived and how many couldn't be used, never which. The helper reads neither:
//! a 200 means the lines were delivered, whatever the site makes of them.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadError {
    /// No answer, or the site had a problem (5xx). Try again later.
    Unreachable,
    /// Too many uploads from this address for now (429). Try again later.
    Busy,
    /// The site refused the upload itself, with this status. Trying the same lines again won't help soon.
    Refused(u16),
}

/// Something that delivers one upload of at most 500 lines.
pub trait Uploader {
    fn upload(&mut self, lines: &[String]) -> Result<(), UploadError>;
}

pub struct HttpUploader {
    http: ureq::Agent,
    api: String,
    user_agent: String,
}

impl HttpUploader {
    /// `api` is where the site's api answers (`https://lfg.plus`).
    pub fn new(api: &str) -> Self {
        let http: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        HttpUploader {
            http,
            api: api.trim_end_matches('/').to_owned(),
            user_agent: format!("LFGPlusHelper/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl Uploader for HttpUploader {
    fn upload(&mut self, lines: &[String]) -> Result<(), UploadError> {
        let response = self
            .http
            .post(format!("{}/api/v1/uploads", self.api))
            .header("User-Agent", &self.user_agent)
            .content_type("application/json")
            .send(request_body(lines))
            .map_err(|_| UploadError::Unreachable)?;
        match response.status().as_u16() {
            200 => Ok(()),
            status => Err(status_error(status)),
        }
    }
}

fn status_error(status: u16) -> UploadError {
    match status {
        429 => UploadError::Busy,
        400..=499 => UploadError::Refused(status),
        _ => UploadError::Unreachable,
    }
}

/// `{"records":[...]}`. Lines are already limited to the record alphabet; quotes and backslashes are escaped anyway.
pub fn request_body(lines: &[String]) -> String {
    let mut body = String::from("{\"records\":[");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            body.push(',');
        }
        body.push('"');
        for c in line.chars() {
            match c {
                '"' => body.push_str("\\\""),
                '\\' => body.push_str("\\\\"),
                c if (c as u32) < 0x20 => body.push_str(&format!("\\u{:04x}", c as u32)),
                c => body.push(c),
            }
        }
        body.push('"');
    }
    body.push_str("]}");
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_request_body() {
        assert_eq!(
            request_body(&["LFGR1:a".into(), "b\"\\\n".into()]),
            "{\"records\":[\"LFGR1:a\",\"b\\\"\\\\\\u000a\"]}"
        );
        assert_eq!(request_body(&[]), "{\"records\":[]}");
    }

    #[test]
    fn sorts_statuses() {
        assert_eq!(status_error(429), UploadError::Busy);
        assert_eq!(status_error(400), UploadError::Refused(400));
        assert_eq!(status_error(503), UploadError::Unreachable);
        assert_eq!(status_error(302), UploadError::Unreachable);
    }
}
