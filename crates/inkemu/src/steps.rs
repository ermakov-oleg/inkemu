//! `--steps` splitting: CPython's `shlex` (posix, `;` punctuation, whitespace split) and the cut at `;`.

/// What `_split_steps` of 0.1 returns: the words of every non-empty step.
pub fn split_steps(s: &str) -> Result<Vec<Vec<String>>, String> {
    let tokens = tokens(s).map_err(|e| format!("cannot parse --steps: {e}"))?;
    let mut out: Vec<Vec<String>> = vec![Vec::new()];
    for t in tokens {
        // shlex glues ";;" into one token; a quoted ";" counts too, as in 0.1
        if !t.is_empty() && t.chars().all(|c| c == ';') {
            out.push(Vec::new());
        } else if let Some(last) = out.last_mut() {
            last.push(t);
        }
    }
    Ok(out.into_iter().filter(|s| !s.is_empty()).collect())
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Start,
    Word,
    Punct,
    Quote(char),
    Escape,
    Eof,
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// `shlex.shlex.read_token` in the configuration above, called until the end of the input.
fn tokens(input: &str) -> Result<Vec<String>, &'static str> {
    let chars: Vec<char> = input.chars().collect();
    let mut pos = 0;
    let mut pushback: Option<char> = None;
    let mut state = State::Start;
    let mut out = Vec::new();
    while state != State::Eof {
        let mut token = String::new();
        let mut quoted = false;
        let mut escaped = State::Start;
        loop {
            let next = match pushback.take() {
                Some(c) => Some(c),
                None => {
                    let c = chars.get(pos).copied();
                    pos += usize::from(c.is_some());
                    c
                }
            };
            match state {
                State::Eof => break,
                State::Start => match next {
                    None => {
                        state = State::Eof;
                        break;
                    }
                    Some(c) if is_space(c) => {
                        if !token.is_empty() || quoted {
                            break;
                        }
                    }
                    Some('#') => skip_line(&chars, &mut pos),
                    Some('\\') => {
                        escaped = State::Word;
                        state = State::Escape;
                    }
                    Some(';') => {
                        token.push(';');
                        state = State::Punct;
                    }
                    Some(c @ ('\'' | '"')) => state = State::Quote(c),
                    Some(c) => {
                        token.push(c);
                        state = State::Word;
                    }
                },
                State::Quote(q) => {
                    quoted = true;
                    match next {
                        None => return Err("No closing quotation"),
                        Some(c) if c == q => state = State::Word,
                        Some('\\') if q == '"' => {
                            escaped = state;
                            state = State::Escape;
                        }
                        Some(c) => token.push(c),
                    }
                }
                State::Escape => match next {
                    None => return Err("No escaped character"),
                    Some(c) => {
                        // inside double quotes only the quote and the backslash itself are escapable
                        if let State::Quote(q) = escaped {
                            if c != '\\' && c != q {
                                token.push('\\');
                            }
                        }
                        token.push(c);
                        state = escaped;
                    }
                },
                State::Word | State::Punct => match next {
                    None => {
                        state = State::Eof;
                        break;
                    }
                    Some(c) if is_space(c) => {
                        state = State::Start;
                        if !token.is_empty() || quoted {
                            break;
                        }
                    }
                    Some('#') => {
                        skip_line(&chars, &mut pos);
                        state = State::Start;
                        if !token.is_empty() || quoted {
                            break;
                        }
                    }
                    Some(c) if state == State::Punct => {
                        if c == ';' {
                            token.push(c);
                        } else {
                            pushback = Some(c);
                            state = State::Start;
                            break;
                        }
                    }
                    Some(c @ ('\'' | '"')) => state = State::Quote(c),
                    Some('\\') => {
                        escaped = State::Word;
                        state = State::Escape;
                    }
                    Some(';') => {
                        pushback = Some(';');
                        state = State::Start;
                        break;
                    }
                    Some(c) => token.push(c),
                },
            }
        }
        if !token.is_empty() || quoted {
            out.push(token);
        }
    }
    Ok(out)
}

fn skip_line(chars: &[char], pos: &mut usize) {
    while let Some(&c) = chars.get(*pos) {
        *pos += 1;
        if c == '\n' {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values come from CPython's `cli._split_steps` on the same inputs.
    #[test]
    #[allow(clippy::type_complexity)]
    fn matches_cpython_shlex() {
        let table: &[(&str, Result<&[&[&str]], &str>)] = &[
            ("", Ok(&[])),
            ("   ", Ok(&[])),
            ("tap 1 2", Ok(&[&["tap", "1", "2"]])),
            (
                "tap 1 2; tap 3 4",
                Ok(&[&["tap", "1", "2"], &["tap", "3", "4"]]),
            ),
            (
                "tap 1 2;tap 3 4",
                Ok(&[&["tap", "1", "2"], &["tap", "3", "4"]]),
            ),
            ("a;b", Ok(&[&["a"], &["b"]])),
            ("a ;b", Ok(&[&["a"], &["b"]])),
            ("a; b", Ok(&[&["a"], &["b"]])),
            (";;", Ok(&[])),
            ("a;;b", Ok(&[&["a"], &["b"]])),
            (";a;", Ok(&[&["a"]])),
            ("a;;;b", Ok(&[&["a"], &["b"]])),
            ("a ; ; b", Ok(&[&["a"], &["b"]])),
            ("'a;b'", Ok(&[&["a;b"]])),
            ("\"a;b\"", Ok(&[&["a;b"]])),
            ("a\\;b", Ok(&[&["a;b"]])),
            ("'a b'", Ok(&[&["a b"]])),
            ("\"a b\"", Ok(&[&["a b"]])),
            ("x\"y z\"w", Ok(&[&["xy zw"]])),
            ("'x'y", Ok(&[&["xy"]])),
            ("\"x\\\"y\"", Ok(&[&["x\"y"]])),
            ("\"x\\ny\"", Ok(&[&["x\\ny"]])),
            ("\"x\\\\y\"", Ok(&[&["x\\y"]])),
            ("'x\\y'", Ok(&[&["x\\y"]])),
            ("a\\ b", Ok(&[&["a b"]])),
            ("a\\\nb", Ok(&[&["a\nb"]])),
            ("\\", Err("cannot parse --steps: No escaped character")),
            ("a\\", Err("cannot parse --steps: No escaped character")),
            ("\"abc", Err("cannot parse --steps: No closing quotation")),
            ("'abc", Err("cannot parse --steps: No closing quotation")),
            ("\"abc\\", Err("cannot parse --steps: No escaped character")),
            ("''", Ok(&[&[""]])),
            ("\"\"", Ok(&[&[""]])),
            ("a '' b", Ok(&[&["a", "", "b"]])),
            ("a ''; b", Ok(&[&["a", ""], &["b"]])),
            ("#c", Ok(&[])),
            ("a #c", Ok(&[&["a"]])),
            ("a#c b", Ok(&[&["a"]])),
            ("a # c\nb", Ok(&[&["a", "b"]])),
            ("a#b;c", Ok(&[&["a"]])),
            ("a;#c", Ok(&[&["a"]])),
            ("a; #c\nb;c", Ok(&[&["a"], &["b"], &["c"]])),
            ("'#a'", Ok(&[&["#a"]])),
            ("\"#a\"", Ok(&[&["#a"]])),
            ("a\\#b", Ok(&[&["a#b"]])),
            ("a\tb", Ok(&[&["a", "b"]])),
            ("a\r\nb", Ok(&[&["a", "b"]])),
            ("a\nb;c\nd", Ok(&[&["a", "b"], &["c", "d"]])),
            (
                "тап 'привет мир'; é;ü",
                Ok(&[&["тап", "привет мир"], &["é"], &["ü"]]),
            ),
            (
                "tap-text 'Tap me'; wait 61s; shot",
                Ok(&[&["tap-text", "Tap me"], &["wait", "61s"], &["shot"]]),
            ),
            ("a;\\;b", Ok(&[&["a"], &[";b"]])),
            ("a\"b\"c;d'e'f", Ok(&[&["abc"], &["def"]])),
            ("\";\"", Ok(&[])),
            ("\\;;", Ok(&[])),
            (";\\a", Ok(&[&["a"]])),
            (";'a'", Ok(&[&["a"]])),
            (";\"a b\"", Ok(&[&["a b"]])),
            ("a;'b c'", Ok(&[&["a"], &["b c"]])),
            ("'a'\"b\"", Ok(&[&["ab"]])),
            ("x\\\"y", Ok(&[&["x\"y"]])),
            ("\"a\\'b\"", Ok(&[&["a\\'b"]])),
            ("\"\\\"\"", Ok(&[&["\""]])),
            ("a ;# x", Ok(&[&["a"]])),
            (";;#x\ny", Ok(&[&["y"]])),
            ("a\\\\;b", Ok(&[&["a\\"], &["b"]])),
            ("type 'it''s'", Ok(&[&["type", "its"]])),
            ("'a\nb'", Ok(&[&["a\nb"]])),
            ("\n\n", Ok(&[])),
            ("a\n;\nb", Ok(&[&["a"], &["b"]])),
            ("\u{a0}a", Ok(&[&["\u{a0}a"]])),
            ("a\u{2028}b", Ok(&[&["a\u{2028}b"]])),
            ("\u{b}a", Ok(&[&["\u{b}a"]])),
            ("😀 x;y", Ok(&[&["😀", "x"], &["y"]])),
        ];
        for (input, want) in table {
            let got = split_steps(input);
            let want: Result<Vec<Vec<String>>, String> = match want {
                Ok(steps) => Ok(steps
                    .iter()
                    .map(|s| s.iter().map(|w| w.to_string()).collect())
                    .collect()),
                Err(e) => Err(e.to_string()),
            };
            assert_eq!(got, want, "input {input:?}");
        }
    }
}
