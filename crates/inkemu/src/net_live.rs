//! `live` and `record`: the real HTTP request for an outgoing `ctx.net:request`, and the fixture a recording
//! writes (0.1 `Net._fetch`, `Net._save`).
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::Duration;

use inkcore::fixtures::{sha256_hex, Redactor};
use inkcore::{Core, NetMode, NetResult, OutRequest, DEFAULT_DELAY_MS};

use crate::secrets::{expand, expand_str, unknown_secret_warnings, urlsplit};

const DEFAULT_TIMEOUT_MS: i64 = 30_000;

pub struct Net {
    mode: NetMode,
    secrets: Vec<(String, String)>,
    redactor: Redactor,
    /// `<fixtures dir>/net`.
    dir: PathBuf,
}

/// 0.1 `SENSITIVE_HEADER`: never written to a fixture.
fn sensitive(name: &str) -> bool {
    let n = name.to_lowercase();
    matches!(
        n.as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie" | "x-api-key"
    ) || ["token", "key", "secret"].iter().any(|w| n.contains(w))
}

/// Python `str.capitalize`, as urllib sends header names.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase()
            .chain(c.flat_map(char::to_lowercase))
            .collect()
    })
}

fn timed_out(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur = Some(e);
    while let Some(e) = cur {
        if let Some(io) = e.downcast_ref::<io::Error>() {
            if matches!(
                io.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ) {
                return true;
            }
        }
        cur = e.source();
    }
    false
}

fn net_error(e: &(dyn std::error::Error + 'static)) -> NetResult {
    NetResult::Error(
        if timed_out(e) {
            "timeout"
        } else {
            "no_network"
        }
        .into(),
    )
}

impl Net {
    pub fn new(mode: NetMode, secrets: Vec<(String, String)>, dir: PathBuf) -> Net {
        Net {
            mode,
            redactor: Redactor::new(&secrets),
            secrets,
            dir,
        }
    }

    /// The same mode and folder with other secrets.
    pub fn with_secrets(&self, secrets: Vec<(String, String)>) -> Net {
        Net::new(self.mode, secrets, self.dir.clone())
    }

    /// Performs one outgoing request: warnings and log lines go to `core`, the answer is returned for
    /// `Core::deliver`.
    pub fn perform(&self, core: &mut Core, r: &OutRequest) -> NetResult {
        let shown = self.redactor.redact_str(&r.url);
        if self.mode == NetMode::Record {
            for w in unknown_secret_warnings(&shown) {
                core.warn(&w);
            }
        }
        let resp = self.fetch(r);
        let outcome = match &resp {
            NetResult::Response { status, .. } => status.to_string(),
            NetResult::Error(e) => e.clone(),
        };
        core.log_line(&format!(
            "net: {} {shown} -> {outcome} ({})",
            r.method,
            self.mode.as_str()
        ));
        if self.mode == NetMode::Record {
            if let NetResult::Response {
                status,
                headers,
                body,
                delay_ms,
            } = &resp
            {
                match self.save(r, *status, headers, body, *delay_ms) {
                    Ok(stem) => core.log_line(&format!("net: recorded net/{stem}.json")),
                    Err(e) => core.warn(&format!(
                        "net: cannot save a fixture in {}: {e}",
                        self.dir.display()
                    )),
                }
            }
        }
        resp
    }

    fn fetch(&self, r: &OutRequest) -> NetResult {
        let url = expand_str(&r.url, &self.secrets);
        // http.client refuses these in the request line, so 0.1 never sent such a URL
        if url.bytes().any(|b| b <= b' ' || b >= 0x7f) {
            return NetResult::Error("no_network".into());
        }
        let timeout = if r.timeout_ms > 0 {
            r.timeout_ms
        } else {
            DEFAULT_TIMEOUT_MS
        };
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(timeout as u64))
            .build();
        let mut req = agent.request(&r.method, &url);
        let mut has_type = false;
        for (k, v) in &r.headers {
            let k = capitalize(k);
            has_type |= k == "Content-type";
            req = req.set(&k, &expand_str(v, &self.secrets));
        }
        let result = if !r.body.is_empty() {
            if !has_type {
                req = req.set("Content-type", "application/x-www-form-urlencoded");
            }
            req.send_bytes(&expand(&r.body, &self.secrets))
        } else if r.method == "POST" {
            req.send_bytes(&[])
        } else {
            req.call()
        };
        let resp = match result {
            Ok(resp) | Err(ureq::Error::Status(_, resp)) => resp,
            Err(ureq::Error::Transport(t)) => return net_error(&t),
        };
        let status = i64::from(resp.status());
        let mut headers: Vec<(String, String)> = Vec::new();
        for name in resp.headers_names() {
            if !headers.iter().any(|(k, _)| *k == name) {
                let value = resp.all(&name).join(", ");
                headers.push((name, value));
            }
        }
        let mut body = Vec::new();
        if let Err(e) = resp.into_reader().read_to_end(&mut body) {
            return net_error(&e);
        }
        NetResult::Response {
            status,
            headers,
            body,
            delay_ms: DEFAULT_DELAY_MS,
        }
    }

    /// 0.1 `Net._stem`: `METHOD_host_hash`.
    fn stem(&self, r: &OutRequest) -> String {
        let shown = self.redactor.redact_str(&r.url);
        let (_, netloc, _, _) = urlsplit(&shown);
        let host = netloc.rsplit('@').next().unwrap_or_default();
        let mut host: String = host
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if host.is_empty() {
            host = "nohost".into();
        }
        let key = self.redactor.fixture_key(&r.method, &r.url, &r.body);
        format!(
            "{}_{host}_{}",
            r.method.to_ascii_uppercase(),
            &sha256_hex(key.as_bytes())[..8]
        )
    }

    fn save(
        &self,
        r: &OutRequest,
        status: i64,
        headers: &[(String, String)],
        body: &[u8],
        delay_ms: i64,
    ) -> io::Result<String> {
        let red = &self.redactor;
        let stem = self.stem(r);
        let sha = red.body_sha(&r.method, &r.body);
        let safe = |h: &[(String, String)], lower: bool| -> Vec<(String, String)> {
            h.iter()
                .filter(|(k, _)| !sensitive(k))
                .map(|(k, v)| {
                    let k = if lower { k.to_lowercase() } else { k.clone() };
                    (k, red.redact_str(v))
                })
                .collect()
        };
        fs::create_dir_all(&self.dir)?;
        fs::write(self.dir.join(format!("{stem}.body")), red.redact(body))?;
        let doc = format!(
            "{{\n  \"request\": {{\n    \"method\": {},\n    \"url\": {},\n    \"body_sha256\": {},\n    \"headers\": {}\n  }},\n  \"match\": \"exact\",\n  \"response\": {{\n    \"status\": {status},\n    \"headers\": {},\n    \"body_file\": {},\n    \"delay_ms\": {delay_ms}\n  }}\n}}\n",
            json_str(&r.method),
            json_str(&red.redact_str(&r.url)),
            sha.as_deref().map_or("null".into(), json_str),
            json_map(&safe(&r.headers, false)),
            json_map(&safe(headers, true)),
            json_str(&format!("{stem}.body")),
        );
        fs::write(self.dir.join(format!("{stem}.json")), doc)?;
        Ok(stem)
    }
}

/// Python `json.dumps(s, ensure_ascii=False)`.
fn json_str(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A string map nested at depth 2 as Python's `json.dumps(indent=2)` writes it.
fn json_map(m: &[(String, String)]) -> String {
    if m.is_empty() {
        return "{}".into();
    }
    let items: Vec<String> = m
        .iter()
        .map(|(k, v)| format!("      {}: {}", json_str(k), json_str(v)))
        .collect();
    format!("{{\n{}\n    }}", items.join(",\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_headers_as_0_1() {
        for h in [
            "Authorization",
            "X-Api-Key",
            "X-My-Token",
            "Set-Cookie",
            "my-secret",
        ] {
            assert!(sensitive(h), "{h}");
        }
        for h in ["User-Agent", "Content-Type", "x-cookie-jar"] {
            assert!(!sensitive(h), "{h}");
        }
    }

    #[test]
    fn header_names_capitalize_as_urllib() {
        assert_eq!(capitalize("X-API-Key"), "X-api-key");
        assert_eq!(capitalize("authorization"), "Authorization");
    }
}
