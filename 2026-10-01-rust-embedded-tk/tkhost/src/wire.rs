//! Tcl list syntax: written by Rust (Rust → Tcl lines), and parsed for
//! values Tcl stores in `rs::ui`.

use anyhow::{Result, bail};
use serde_json::Value;

/// Quote `s` as one list element. Backslash-escaping every special
/// character is always valid list syntax, and newlines become `\n`, so the
/// result never spans lines.
pub fn quote(s: &str) -> String {
    if s.is_empty() {
        return "{}".into();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '\x0c' => out.push_str("\\f"),
            '\\' | '{' | '}' | '[' | ']' | '$' | '"' | ';' | ' ' | '#' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// A Tcl list of these strings.
pub fn list<S: AsRef<str>>(items: impl IntoIterator<Item = S>) -> String {
    items.into_iter().map(|s| quote(s.as_ref())).collect::<Vec<_>>().join(" ")
}

/// Objects become dicts, arrays become lists, scalars become strings,
/// `null` becomes the empty string.
pub fn to_tcl(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => (if *b { "1" } else { "0" }).into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(a) => list(a.iter().map(to_tcl)),
        Value::Object(o) => list(o.iter().flat_map(|(k, v)| [k.clone(), to_tcl(v)])),
    }
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c')
}

/// Parse a Tcl list into its elements (like `Tcl_SplitList`).
pub fn parse_list(s: &str) -> Result<Vec<String>> {
    let mut out = vec![];
    let mut it = s.chars().peekable();
    loop {
        while it.next_if(|&c| is_space(c)).is_some() {}
        let Some(&first) = it.peek() else { return Ok(out) };
        let mut elem = String::new();
        match first {
            '{' => {
                it.next();
                let mut depth = 1;
                loop {
                    match it.next() {
                        None => bail!("unmatched open brace in list"),
                        Some('\\') => {
                            elem.push('\\');
                            if let Some(c) = it.next() {
                                elem.push(c);
                            }
                        }
                        Some('{') => {
                            depth += 1;
                            elem.push('{');
                        }
                        Some('}') => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            elem.push('}');
                        }
                        Some(c) => elem.push(c),
                    }
                }
            }
            '"' => {
                it.next();
                loop {
                    match it.next() {
                        None => bail!("unmatched open quote in list"),
                        Some('"') => break,
                        Some('\\') => backslash(&mut it, &mut elem),
                        Some(c) => elem.push(c),
                    }
                }
            }
            _ => {
                while let Some(c) = it.next_if(|&c| !is_space(c)) {
                    if c == '\\' { backslash(&mut it, &mut elem) } else { elem.push(c) }
                }
                out.push(elem);
                continue;
            }
        }
        if it.peek().is_some_and(|&c| !is_space(c)) {
            bail!("list element in braces or quotes followed by garbage");
        }
        out.push(elem);
    }
}

/// The character(s) after a backslash, substituted.
fn backslash(it: &mut std::iter::Peekable<std::str::Chars>, out: &mut String) {
    let Some(c) = it.next() else {
        out.push('\\');
        return;
    };
    let hex = |it: &mut std::iter::Peekable<std::str::Chars>, max| {
        let mut n = 0u32;
        let mut digits = 0;
        while digits < max {
            match it.peek().and_then(|c| c.to_digit(16)) {
                Some(d) => {
                    n = n * 16 + d;
                    digits += 1;
                    it.next();
                }
                None => break,
            }
        }
        (digits > 0).then_some(n)
    };
    match c {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        'v' => out.push('\x0b'),
        'f' => out.push('\x0c'),
        'a' => out.push('\x07'),
        'b' => out.push('\x08'),
        'x' | 'u' | 'U' => {
            let max = match c { 'x' => 2, 'u' => 4, _ => 8 };
            match hex(it, max) {
                Some(n) => out.push(char::from_u32(n).unwrap_or('\u{fffd}')),
                None => out.push(c),
            }
        }
        '\n' => {
            while it.next_if(|&c| c == ' ' || c == '\t').is_some() {}
            out.push(' ');
        }
        c => out.push(c),
    }
}
