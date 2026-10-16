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

/// Where a value sits in a JSONC text: the byte range it spans in the
/// text as written (comments, BOM and all), and — for an object or an
/// array — where its members do. The OUTLINE is what an edit that must
/// keep a file's comments and layout goes by (the TUI's `wt_settings`): the
/// change is spliced in at a range found here, and the rest of the text is
/// left byte for byte as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// `start..end` in bytes: from the value's first character to just
    /// past its last.
    pub start: usize,
    pub end: usize,
    pub shape: Shape,
}

/// What a [`Node`] holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// The members, in the order written, each key with its value's node.
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    /// A string, a number, `true`, `false` or `null`.
    Scalar,
}

impl Node {
    /// The value under `key`, when this is an object that has one.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.shape {
            Shape::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// The OUTLINE of a JSONC text (see [`Node`]), or why it has none.
///
/// It walks [`strip`]'s output, which is plain JSON lined up char for char
/// with the text as written — a comment's every char became one space or
/// newline, a trailing comma a space — so a position found there is a
/// position in the original once it is turned back from chars to bytes.
pub fn outline(text: &str) -> Result<Node, String> {
    let bom = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let body = &text[bom..];
    let plain: Vec<char> = strip(body).chars().collect();
    // Char index (in `body`) to byte offset (in `text`), one past the end
    // included.
    let mut bytes: Vec<usize> = body.char_indices().map(|(b, _)| b + bom).collect();
    bytes.push(text.len());
    if bytes.len() != plain.len() + 1 {
        return Err("the comments did not line up".into());
    }
    let mut walk = Walk { s: &plain, i: 0 };
    let node = walk.value()?;
    walk.space();
    if walk.i != plain.len() {
        return Err(format!("text after the value at char {}", walk.i));
    }
    Ok(rebase(node, &bytes))
}

/// [`outline`]'s nodes, from char indices to byte offsets.
fn rebase(node: Node, bytes: &[usize]) -> Node {
    let shape = match node.shape {
        Shape::Object(members) => Shape::Object(
            members
                .into_iter()
                .map(|(k, v)| (k, rebase(v, bytes)))
                .collect(),
        ),
        Shape::Array(items) => Shape::Array(items.into_iter().map(|v| rebase(v, bytes)).collect()),
        Shape::Scalar => Shape::Scalar,
    };
    Node {
        start: bytes[node.start],
        end: bytes[node.end],
        shape,
    }
}

/// A recursive-descent walk over plain JSON, in chars.
struct Walk<'a> {
    s: &'a [char],
    i: usize,
}

impl Walk<'_> {
    fn space(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        self.space();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected `{c}` at char {}", self.i))
        }
    }

    fn value(&mut self) -> Result<Node, String> {
        self.space();
        let start = self.i;
        let shape = match self.s.get(self.i) {
            Some('{') => {
                self.i += 1;
                let mut members = Vec::new();
                self.space();
                if self.s.get(self.i) == Some(&'}') {
                    self.i += 1;
                } else {
                    loop {
                        self.space();
                        let key_at = self.i;
                        self.string()?;
                        let key: String = self.s[key_at..self.i].iter().collect();
                        let key =
                            serde_json::from_str::<String>(&key).map_err(|e| e.to_string())?;
                        self.expect(':')?;
                        members.push((key, self.value()?));
                        self.space();
                        match self.s.get(self.i) {
                            Some(',') => self.i += 1,
                            Some('}') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(format!("expected `,` or `}}` at char {}", self.i)),
                        }
                    }
                }
                Shape::Object(members)
            }
            Some('[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.space();
                if self.s.get(self.i) == Some(&']') {
                    self.i += 1;
                } else {
                    loop {
                        items.push(self.value()?);
                        self.space();
                        match self.s.get(self.i) {
                            Some(',') => self.i += 1,
                            Some(']') => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(format!("expected `,` or `]` at char {}", self.i)),
                        }
                    }
                }
                Shape::Array(items)
            }
            Some('"') => {
                self.string()?;
                Shape::Scalar
            }
            Some(_) => {
                // A number or a literal: up to the next delimiter.
                while self
                    .s
                    .get(self.i)
                    .is_some_and(|c| !c.is_whitespace() && !matches!(c, ',' | '}' | ']' | ':'))
                {
                    self.i += 1;
                }
                if self.i == start {
                    return Err(format!("expected a value at char {start}"));
                }
                Shape::Scalar
            }
            None => return Err("expected a value at the end".into()),
        };
        Ok(Node {
            start,
            end: self.i,
            shape,
        })
    }

    /// Past a string literal, escapes included.
    fn string(&mut self) -> Result<(), String> {
        if self.s.get(self.i) != Some(&'"') {
            return Err(format!("expected a string at char {}", self.i));
        }
        self.i += 1;
        while let Some(&c) = self.s.get(self.i) {
            self.i += 1;
            match c {
                '\\' => self.i += 1,
                '"' => return Ok(()),
                _ => {}
            }
        }
        Err("a string runs off the end".into())
    }
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

    /// The outline's ranges land on the values as written — past a BOM,
    /// comments with non-ASCII in them and trailing commas — so slicing
    /// the original text at them gives each value back verbatim.
    #[test]
    fn the_outline_points_into_the_text_as_written() {
        let text = "\u{feff}{\n  // café ☕\n  \"a\": [1, /* é */ {\"k\": \"v\"},],\n  \"b\": \"x\\\"y\",\n}";
        let root = outline(text).unwrap();
        assert_eq!(&text[root.start..root.end], &text[3..]);
        let a = root.get("a").unwrap();
        assert_eq!(&text[a.start..a.end], "[1, /* é */ {\"k\": \"v\"},]");
        let Shape::Array(items) = &a.shape else {
            panic!("an array")
        };
        assert_eq!(&text[items[1].start..items[1].end], "{\"k\": \"v\"}");
        let b = root.get("b").unwrap();
        assert_eq!(&text[b.start..b.end], "\"x\\\"y\"");
        assert!(root.get("c").is_none());
        assert!(outline("{\"a\": }").is_err());
        assert!(outline("{} x").is_err());
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
