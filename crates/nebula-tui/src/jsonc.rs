//! JSON WITH COMMENTS: the dialect VS Code's `settings.json`, its theme
//! files and Windows Terminal's `settings.json` are all written in —
//! `//` and `/* */` comments, and a comma left trailing before a `}` or
//! `]`. [`strip`] turns it into plain JSON that `serde_json` reads, so the
//! one parser stays the one parser; [`parse`] does both.
//!
//! The stripper walks the text once, knowing when it is inside a string
//! (escapes included), so a `//` in a URL or a `,]` in a string literal is
//! left exactly as written.

/// `text` with its comments blanked out and its trailing commas dropped:
/// plain JSON, everything else byte for byte as it was. A comment becomes
/// spaces (a line comment keeps its newline), so a `serde_json` error
/// still points at the line it was on in the original.
pub fn strip(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    // Where the last comma written outside a string sits in `out`, while
    // nothing but whitespace and comments has followed it.
    let mut pending_comma: Option<usize> = None;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                pending_comma = None;
                out.push(c);
                i += 1;
                while i < chars.len() {
                    let s = chars[i];
                    out.push(s);
                    i += 1;
                    if s == '\\' {
                        if let Some(&next) = chars.get(i) {
                            out.push(next);
                            i += 1;
                        }
                    } else if s == '"' {
                        break;
                    }
                }
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(' ');
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                out.push_str("  ");
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                if i < chars.len() {
                    out.push_str("  ");
                    i += 2;
                }
            }
            ',' => {
                pending_comma = Some(out.len());
                out.push(c);
                i += 1;
            }
            '}' | ']' => {
                if let Some(at) = pending_comma.take() {
                    out.replace_range(at..at + 1, " ");
                }
                out.push(c);
                i += 1;
            }
            c if c.is_whitespace() => {
                out.push(c);
                i += 1;
            }
            _ => {
                pending_comma = None;
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Parse JSON with comments into a `serde_json::Value`.
pub fn parse(text: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(&strip(text)).map_err(|e| e.to_string())
}

/// Read and parse a JSONC file; None when it is missing or unreadable.
pub fn read(path: &std::path::Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    parse(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Comments of both kinds and trailing commas go; what is in a string
    /// stays exactly as written, `//` and `,]` included.
    #[test]
    fn comments_and_trailing_commas_go_strings_stay() {
        let text = r#"{
            // a line comment
            "url": "https://example.com/a//b", /* a block
               comment */
            "list": [1, 2, 3,],
            "odd": "a,]b \"quoted\" // not a comment",
            "nested": { "x": true, },
        }"#;
        assert_eq!(
            parse(text).unwrap(),
            json!({
                "url": "https://example.com/a//b",
                "list": [1, 2, 3],
                "odd": "a,]b \"quoted\" // not a comment",
                "nested": { "x": true },
            })
        );
    }

    /// A byte-order mark at the front (Windows editors write one) is not
    /// JSON either.
    #[test]
    fn a_byte_order_mark_is_dropped() {
        assert_eq!(parse("\u{feff}{\"a\": 1}").unwrap(), json!({"a": 1}));
    }

    /// A comma between two values is a comma, not a trailing one.
    #[test]
    fn an_inner_comma_stays() {
        assert_eq!(
            parse("[1, // one\n 2]").unwrap(),
            json!([1, 2]),
            "a comment after an inner comma"
        );
    }
}
