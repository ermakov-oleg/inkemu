//! Network fixtures and secret redaction (0.1 `net.Net`): the pure parts the core needs to answer `replay`
//! requests and that the native shell reuses to record them.
use std::collections::BTreeMap;

use serde_json::Value as J;
use sha2::{Digest, Sha256};

use crate::ctx::net::{NetResult, DEFAULT_DELAY_MS};
use crate::log::py_str;

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A secret value in one of its spellings and the `{{name}}` that replaces it.
struct Variant {
    text: Vec<u8>,
    /// Positions inside a `%XX` escape that match in either case.
    any_case: Vec<bool>,
    placeholder: Vec<u8>,
}

impl Variant {
    fn matches_at(&self, data: &[u8], i: usize) -> bool {
        let Some(window) = data.get(i..i + self.text.len()) else {
            return false;
        };
        window
            .iter()
            .zip(&self.text)
            .zip(&self.any_case)
            .all(|((&d, &t), &ci)| d == t || (ci && d.eq_ignore_ascii_case(&t)))
    }
}

/// Replaces secret values with `{{name}}` and URL passwords with `{{userinfo}}` (0.1 `Net.redact`).
#[derive(Default)]
pub struct Redactor {
    variants: Vec<Variant>,
}

impl Redactor {
    /// `secrets` as (name, non-empty value), in the order of the secrets file.
    pub fn new(secrets: &[(String, String)]) -> Redactor {
        let mut pairs: Vec<(Vec<u8>, String)> = Vec::new();
        for (name, value) in secrets {
            if value.is_empty() {
                continue;
            }
            for v in [
                value.clone(),
                quote(value, b""),
                quote(value, b"@/:"),
                quote_plus(value),
            ] {
                if !pairs.iter().any(|(t, _)| t == v.as_bytes()) {
                    pairs.push((v.into_bytes(), format!("{{{{{name}}}}}")));
                }
            }
        }
        // longest first, so a secret containing another wins; stable like Python's sort
        pairs.sort_by_key(|(t, _)| std::cmp::Reverse(t.len()));
        let variants = pairs
            .into_iter()
            .map(|(text, placeholder)| Variant {
                any_case: escape_mask(&text),
                text,
                placeholder: placeholder.into_bytes(),
            })
            .collect();
        Redactor { variants }
    }

    pub fn redact(&self, data: &[u8]) -> Vec<u8> {
        let mut data = data.to_vec();
        for v in &self.variants {
            if v.text.is_empty() {
                continue;
            }
            let mut out = Vec::with_capacity(data.len());
            let mut i = 0;
            while i < data.len() {
                if v.matches_at(&data, i) {
                    out.extend_from_slice(&v.placeholder);
                    i += v.text.len();
                } else {
                    out.push(data[i]);
                    i += 1;
                }
            }
            data = out;
        }
        redact_userinfo(&data)
    }

    pub fn redact_str(&self, text: &str) -> String {
        // only whole UTF-8 sequences are replaced, by ASCII
        String::from_utf8_lossy(&self.redact(text.as_bytes())).into_owned()
    }

    /// 0.1 `Net.fixture_key`: method, redacted URL and, for a request with a body, the redacted body's SHA-256.
    pub fn fixture_key(&self, method: &str, url: &str, body: &[u8]) -> String {
        let method = method.to_ascii_uppercase();
        let mut key = format!("{method} {}", self.redact_str(url));
        if let Some(sha) = self.body_sha(body) {
            key.push(' ');
            key.push_str(&sha);
        }
        key
    }

    /// The SHA-256 a request body is matched by, whatever the method (0.1: POST only); None for an empty body.
    pub fn body_sha(&self, body: &[u8]) -> Option<String> {
        (!body.is_empty()).then(|| sha256_hex(&self.redact(body)))
    }
}

/// True at the letters of each `%XX` escape (uppercase hex, as `quote` writes them).
fn escape_mask(text: &[u8]) -> Vec<bool> {
    let mut mask = vec![false; text.len()];
    let upper_hex = |b: u8| b.is_ascii_digit() || (b'A'..=b'F').contains(&b);
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'%' && i + 2 < text.len() && upper_hex(text[i + 1]) && upper_hex(text[i + 2])
        {
            mask[i + 1] = text[i + 1].is_ascii_alphabetic();
            mask[i + 2] = text[i + 2].is_ascii_alphabetic();
            i += 3;
            continue;
        }
        i += 1;
    }
    mask
}

/// Python `urllib.parse.quote(s, safe)`.
fn quote(s: &str, safe: &[u8]) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"_.-~".contains(&b) || safe.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Python `urllib.parse.quote_plus(s, safe="")`.
fn quote_plus(s: &str) -> String {
    if s.contains(' ') {
        quote(s, b" ").replace(' ', "+")
    } else {
        quote(s, b"")
    }
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Bytes a URL's userinfo cannot contain; quotes, commas and brackets end a URL in JSON/HTML bodies.
fn ends_userinfo(b: u8) -> bool {
    b.is_ascii_whitespace() || b"/?#\"',<>`@\x0b".contains(&b)
}

/// 0.1 `_USERINFO_PASSWORD`: `scheme://user:password@` becomes `scheme://user:{{userinfo}}@`.
fn redact_userinfo(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if !is_word(data[i]) {
            out.push(data[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < data.len() && is_word(data[j]) {
            j += 1;
        }
        if let Some(end) = userinfo_after(data, j) {
            // keep `scheme://user`, replace the password
            out.extend_from_slice(&data[i..end.0]);
            out.extend_from_slice(b":{{userinfo}}@");
            i = end.1;
        } else {
            out.extend_from_slice(&data[i..j]);
            i = j;
        }
    }
    out
}

/// With `j` just after the scheme: (end of the user part, end of the match) when `://user:password@` follows.
fn userinfo_after(data: &[u8], j: usize) -> Option<(usize, usize)> {
    if !data[j..].starts_with(b"://") {
        return None;
    }
    let mut k = j + 3;
    while k < data.len() && !ends_userinfo(data[k]) && data[k] != b':' {
        k += 1;
    }
    if data.get(k) != Some(&b':') {
        return None;
    }
    let user_end = k;
    k += 1;
    while k < data.len() && !ends_userinfo(data[k]) {
        k += 1;
    }
    (data.get(k) == Some(&b'@')).then_some((user_end, k + 1))
}

/// One parsed fixture file, or why it cannot be used.
struct Fixture {
    name: String,
    doc: Result<J, String>,
}

/// The fixtures of `<fixtures dir>/net/`, as 0.1 `Net._find` reads them.
#[derive(Default)]
pub struct FixtureSet {
    files: BTreeMap<String, Vec<u8>>,
    fixtures: Vec<Fixture>,
}

impl FixtureSet {
    /// Every file of the folder by name: the `.json` fixtures and the body files they name.
    pub fn from_files(files: BTreeMap<String, Vec<u8>>) -> FixtureSet {
        let fixtures = files
            .iter()
            .filter(|(name, _)| name.ends_with(".json"))
            .map(|(name, data)| Fixture {
                name: name.clone(),
                doc: parse(data),
            })
            .collect();
        FixtureSet { files, fixtures }
    }

    /// The answer for a request; malformed fixtures are reported through `warn` on every lookup, as 0.1.
    pub fn find(
        &self,
        method: &str,
        shown: &str,
        sha: Option<&str>,
        warn: &mut dyn FnMut(String),
    ) -> Option<NetResult> {
        let mut best: Option<(bool, &Fixture, &J)> = None;
        for f in &self.fixtures {
            let d = match &f.doc {
                Ok(d) => d,
                Err(problem) => {
                    warn(format!(
                        "net: fixture {} is not valid ({problem}), ignored",
                        f.name
                    ));
                    continue;
                }
            };
            let req = &d["request"];
            if py_str(&req["method"]).to_uppercase() != method {
                continue;
            }
            let u = req["url"].as_str().unwrap_or_default();
            let how = d.get("match").and_then(J::as_str).unwrap_or("exact");
            let ok = match how {
                "prefix" => shown.starts_with(u),
                "glob" => fnmatch(shown, u),
                _ => shown == u && req.get("body_sha256").and_then(J::as_str) == sha,
            };
            // the first exact match wins, else the first other one
            if ok && best.as_ref().is_none_or(|b| b.0 && how == "exact") {
                best = Some((how != "exact", f, d));
            }
        }
        let (_, f, d) = best?;
        Some(self.response(f, &d["response"], warn))
    }

    fn response(&self, f: &Fixture, r: &J, warn: &mut dyn FnMut(String)) -> NetResult {
        let body = match r
            .get("body_file")
            .and_then(J::as_str)
            .filter(|s| !s.is_empty())
        {
            Some(body_file) => {
                let name = path_name(body_file);
                self.files.get(name).cloned().unwrap_or_else(|| {
                    warn(format!(
                        "net: fixture {}: body_file {name} not found, the body is empty",
                        f.name
                    ));
                    Vec::new()
                })
            }
            None => r
                .get("body")
                .and_then(J::as_str)
                .unwrap_or_default()
                .as_bytes()
                .to_vec(),
        };
        let headers = r
            .get("headers")
            .and_then(J::as_object)
            .map(|h| {
                h.iter()
                    .map(|(k, v)| (k.to_lowercase(), py_str(v)))
                    .collect()
            })
            .unwrap_or_default();
        let status = r.get("status").map_or(200, |s| {
            s.as_i64()
                .unwrap_or_else(|| if s.as_u64().is_some() { i64::MAX } else { 200 })
        });
        let delay_ms = r
            .get("delay_ms")
            .map_or(DEFAULT_DELAY_MS, |d| match d.as_i64() {
                Some(n) => n,
                None => d.as_f64().map_or(i64::MAX, |x| x.round_ties_even() as i64),
            });
        NetResult::Response {
            status,
            headers,
            body,
            delay_ms,
        }
    }
}

fn parse(data: &[u8]) -> Result<J, String> {
    let Ok(text) = std::str::from_utf8(data) else {
        return Err("cannot be read (UnicodeDecodeError)".into());
    };
    let d: J = serde_json::from_str(text).map_err(|_| "cannot be read (JSONDecodeError)")?;
    match fixture_problem(&d) {
        Some(p) => Err(p.into()),
        None => Ok(d),
    }
}

/// Python `isinstance(v, (int, float)) and not isinstance(v, bool)`.
fn number(v: &J) -> bool {
    v.is_number()
}

/// 0.1 `_fixture_problem`: why a hand-written fixture cannot be used, or None. A key given as null is not the
/// same as a missing one, as in Python's `dict.get(key, default)`.
fn fixture_problem(d: &J) -> Option<&'static str> {
    let (Some(req), Some(resp)) = (
        d.get("request").filter(|v| v.is_object()),
        d.get("response").filter(|v| v.is_object()),
    ) else {
        return Some(r#"needs "request" and "response" objects"#);
    };
    if !req.get("method").is_some_and(J::is_string) || !req.get("url").is_some_and(J::is_string) {
        return Some("request.method and request.url must be strings");
    }
    if req
        .get("body_sha256")
        .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return Some("request.body_sha256 must be a string");
    }
    if !d
        .get("match")
        .is_none_or(|m| matches!(m.as_str(), Some("exact" | "prefix" | "glob")))
    {
        return Some("match must be exact, prefix or glob");
    }
    if !resp.get("status").is_none_or(|s| s.is_i64() || s.is_u64()) {
        return Some("response.status must be an integer");
    }
    if !resp
        .get("delay_ms")
        .is_none_or(|v| number(v) && v.as_f64().is_some_and(|x| x >= 0.0 && x.is_finite()))
    {
        return Some("response.delay_ms must be a number >= 0");
    }
    if resp
        .get("headers")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Some("response.headers must be an object");
    }
    if resp
        .get("body")
        .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return Some("response.body must be a string");
    }
    if resp
        .get("body_file")
        .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return Some("response.body_file must be a string");
    }
    None
}

/// Python `PurePosixPath(p).name`.
fn path_name(p: &str) -> &str {
    p.split('/')
        .rfind(|s| !s.is_empty() && *s != ".")
        .unwrap_or("")
}

enum Glob {
    Star,
    Any,
    Lit(char),
    /// (ranges, negated)
    Class(Vec<(char, char)>, bool),
}

/// Python `fnmatch.fnmatchcase`: `*` and `?` cross `/`, `[...]` and `[!...]` sets.
pub fn fnmatch(name: &str, pat: &str) -> bool {
    let toks = glob_tokens(pat);
    let name: Vec<char> = name.chars().collect();
    let one = |t: &Glob, c: char| match t {
        Glob::Any => true,
        Glob::Lit(l) => *l == c,
        Glob::Class(ranges, neg) => ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi) != *neg,
        Glob::Star => false,
    };
    // classic wildcard matching with one backtrack point for the last star
    let (mut t, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if t < toks.len() && matches!(toks[t], Glob::Star) {
            star = Some((t, n));
            t += 1;
        } else if t < toks.len() && one(&toks[t], name[n]) {
            t += 1;
            n += 1;
        } else if let Some((st, sn)) = star {
            t = st + 1;
            n = sn + 1;
            star = Some((st, sn + 1));
        } else {
            return false;
        }
    }
    toks[t..].iter().all(|t| matches!(t, Glob::Star))
}

fn glob_tokens(pat: &str) -> Vec<Glob> {
    let p: Vec<char> = pat.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < p.len() {
        let c = p[i];
        i += 1;
        match c {
            '*' => {
                if !matches!(out.last(), Some(Glob::Star)) {
                    out.push(Glob::Star);
                }
            }
            '?' => out.push(Glob::Any),
            '[' => {
                let mut j = i;
                if j < p.len() && p[j] == '!' {
                    j += 1;
                }
                if j < p.len() && p[j] == ']' {
                    j += 1;
                }
                while j < p.len() && p[j] != ']' {
                    j += 1;
                }
                if j >= p.len() {
                    out.push(Glob::Lit('['));
                    continue;
                }
                let mut set = &p[i..j];
                i = j + 1;
                let neg = set.first() == Some(&'!');
                if neg {
                    set = &set[1..];
                }
                let mut ranges = Vec::new();
                let mut k = 0;
                while k < set.len() {
                    if k + 2 < set.len() && set[k + 1] == '-' {
                        if set[k] <= set[k + 2] {
                            ranges.push((set[k], set[k + 2]));
                        }
                        k += 3;
                    } else {
                        ranges.push((set[k], set[k]));
                        k += 1;
                    }
                }
                if ranges.is_empty() && neg {
                    out.push(Glob::Any);
                } else {
                    out.push(Glob::Class(ranges, neg));
                }
            }
            c => out.push(Glob::Lit(c)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(secrets: &[(&str, &str)]) -> Redactor {
        Redactor::new(
            &secrets
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn encoded_forms_redact_in_either_case() {
        let red = r(&[("pw", "pa ss/wd+x")]);
        for shown in [
            "pa+ss%2Fwd%2Bx",
            "pa%20ss%2fwd%2bx",
            "pa%20ss%2Fwd%2Bx",
            "pa ss/wd+x",
        ] {
            assert_eq!(
                red.redact_str(&format!("http://h.test/a/{shown}/b")),
                "http://h.test/a/{{pw}}/b"
            );
            assert_eq!(red.redact(format!("x={shown};").as_bytes()), b"x={{pw}};");
        }
    }

    #[test]
    fn userinfo_passwords_only() {
        let red = Redactor::default();
        assert_eq!(
            red.redact_str("https://bob:hunter2@h.test/x?q=1"),
            "https://bob:{{userinfo}}@h.test/x?q=1"
        );
        assert_eq!(
            red.redact_str("https://bob@h.test/x"),
            "https://bob@h.test/x"
        );
        assert_eq!(
            red.fixture_key("get", "https://bob:hunter2@h.test/x", b""),
            "GET https://bob:{{userinfo}}@h.test/x"
        );
        for text in [
            r#"{"site":"https://example.com","email":"a@b.c"}"#,
            r#"https://h:8080","m":"x@y"#,
            "<a href='https://h.test:1/'>x@y</a>",
        ] {
            assert_eq!(red.redact_str(text), text);
        }
    }

    #[test]
    fn keys_carry_the_body_of_any_method() {
        let red = Redactor::default();
        let sha = sha256_hex(b"<a/>");
        assert_eq!(
            red.fixture_key("POST", "http://h.test/c", b"<a/>"),
            format!("POST http://h.test/c {sha}")
        );
        assert_eq!(
            red.fixture_key("REPORT", "http://h.test/c", b"<a/>"),
            format!("REPORT http://h.test/c {sha}")
        );
        assert_ne!(
            red.fixture_key("PUT", "http://h.test/c", b"<a/>"),
            red.fixture_key("PUT", "http://h.test/c", b"<b/>")
        );
        assert_eq!(
            red.fixture_key("POST", "http://h.test/c", b""),
            "POST http://h.test/c"
        );
    }

    #[test]
    fn globs_as_fnmatchcase() {
        assert!(fnmatch("http://x.test/errors/1", "http://x.test/err*"));
        assert!(fnmatch("a/b", "a?b"));
        assert!(fnmatch("ab1", "ab[0-9]"));
        assert!(!fnmatch("abx", "ab[0-9]"));
        assert!(fnmatch("abx", "ab[!0-9]"));
        assert!(fnmatch("a[", "a["));
        assert!(!fnmatch("A", "a"));
        assert!(fnmatch("aXbXc", "a*b*c"));
        assert!(!fnmatch("aXbX", "a*b*c"));
    }
}
