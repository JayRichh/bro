//! The DIFF VIEWER's model: a unified diff read into hunks, lines and
//! CHANGES, and laid out in one of three DIFF LAYOUTS (Settings →
//! Appearance → **Diff layout**, `Ctrl+s` in the viewer):
//!
//! - **side by side** — the old file left, the new one right, each line
//!   numbered on its side, a removed line facing what replaced it;
//! - **inline** — one column, with an old and a new line-number gutter;
//! - **replacement** — the new text only: a changed line shows its
//!   changed words lit, and what was removed outright folds into a
//!   `− 3 lines removed` marker where it was.
//!
//! In every layout a replaced line's words are diffed against the line it
//! replaced ([`word_ranges`], VS Code's "inline diff" — the line wears a
//! faint add or remove background and the words that changed a stronger
//! one), and every line keeps its syntax highlighting, read per side so a
//! removed line is colored as the old file was and an added one as the
//! new file is.
//!
//! A CHANGE is one run of removed and added lines with no unchanged line
//! between them — what `Ctrl+n`/`Ctrl+p` step to and what `Ctrl+y`
//! stages and `Ctrl+x` discards. It is finer than git's hunk (which
//! gathers changes within three lines of each other): the unit VS Code's
//! gutter stages. The patch for one is cut from a zero-context diff
//! (`git diff -U0`) and applied with `--unidiff-zero` ([`RawHunk`]).
//!
//! Classification and layout live here; colors and spans in `ui::overlay`
//! (the `classify_diff_line` split).

use crate::git_diff::DiffLineKind;
use crate::syntax::{Highlighter, TokenKind};

/// A tab, as the diff pane draws one.
const TAB: &str = "    ";

/// How a DIFF VIEWER lays a diff out (the module docs). The words it
/// stores are shared with the desktop window, so the enum lives there.
pub use nebula_config::choices::DiffMode;

/// Which side of the diff a line is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Context,
    Add,
    Remove,
}

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: Kind,
    /// Its number in the old file (context and removed lines).
    pub old: Option<u32>,
    /// Its number in the new file (context and added lines).
    pub new: Option<u32>,
    /// The text, tabs expanded, without the `+`/`-`/` ` marker.
    pub text: String,
    /// `text` as syntax runs, highlighted with its own side's file.
    pub tokens: Vec<(TokenKind, String)>,
    /// The line it replaced or was replaced by, for the word diff: the
    /// n-th removed line of a CHANGE pairs with its n-th added line.
    pub partner: Option<usize>,
    /// The CHANGE it belongs to; None for an unchanged line.
    pub change: Option<usize>,
}

/// One `@@` hunk: its header as git wrote it, its ranges, and its lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    /// Unchanged lines between the end of the hunk before (or the top of
    /// the file) and this one: what the diff left out above it.
    pub hidden: u32,
}

/// One CHANGE: a run of removed and added lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub hunk: usize,
    /// Its lines, as indexes into [`DiffDoc::lines`].
    pub first: usize,
    pub end: usize,
    pub removed: u32,
    pub added: u32,
    /// The new file's first added line, or — for a change that only
    /// removes — the line after which the removal happened (git's own
    /// `+n,0`).
    pub new_anchor: u32,
}

impl Change {
    /// The new-file lines it covers, as `(start, len)`.
    pub fn new_range(&self) -> (u32, u32) {
        (self.new_anchor, self.added)
    }
}

/// What a diff reads as, before any layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A line about the file (a rename, a mode change, `\ No newline at
    /// end of file`), drawn across the pane.
    Note(String),
    Hunk(usize),
    Line(usize),
}

/// A diff read into hunks, lines and changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiffDoc {
    pub items: Vec<Item>,
    pub hunks: Vec<Hunk>,
    pub lines: Vec<Line>,
    pub changes: Vec<Change>,
    /// The diff was read with the whole file as context
    /// (`git diff -U<huge>`): nothing is hidden, and the hunk headers are
    /// noise.
    pub full: bool,
}

impl DiffDoc {
    /// Read the unified diff `text` of the file at `path` (for its
    /// language). The `diff --git` / `index` / `---` / `+++` headers say
    /// nothing the pane's title does not, and are dropped. None for text
    /// with no hunk in it (a summary, a status, an error), which the pane
    /// shows as it is.
    pub fn parse(text: &str, path: &str, full: bool) -> Option<Self> {
        if !text.lines().any(|l| l.starts_with("@@")) {
            return None;
        }
        let mut doc = DiffDoc {
            full,
            ..DiffDoc::default()
        };
        let (mut old, mut new) = (0u32, 0u32);
        let mut in_hunk = false;
        let mut prev_end = 1u32;
        let mut old_hl = Highlighter::for_path(path);
        let mut new_hl = Highlighter::for_path(path);
        // The open change: its start in `lines` and the new-file line
        // before its first line.
        let mut open: Option<(usize, u32)> = None;
        for raw in text.lines() {
            let raw = raw.strip_suffix('\r').unwrap_or(raw);
            if raw.starts_with("@@") {
                doc.close_change(&mut open);
                let (os, ol, ns, nl) = parse_header(raw).unwrap_or((1, 0, 1, 0));
                let hidden = if os == 0 {
                    0
                } else {
                    os.saturating_sub(prev_end)
                };
                prev_end = os + ol;
                old = os;
                new = ns;
                doc.hunks.push(Hunk {
                    header: raw.to_string(),
                    old_start: os,
                    old_len: ol,
                    new_start: ns,
                    new_len: nl,
                    hidden,
                });
                doc.items.push(Item::Hunk(doc.hunks.len() - 1));
                // Highlighting starts over at each hunk: what came before
                // it is not in the diff to be read.
                old_hl = Highlighter::for_path(path);
                new_hl = Highlighter::for_path(path);
                in_hunk = true;
            } else if raw.starts_with("diff --git") {
                doc.close_change(&mut open);
                in_hunk = false;
            } else if !in_hunk {
                const DROPPED: [&str; 3] = ["index ", "--- ", "+++ "];
                if !DROPPED.iter().any(|d| raw.starts_with(d)) {
                    doc.items.push(Item::Note(raw.to_string()));
                }
            } else if let Some(text) = raw.strip_prefix('-') {
                let text = text.replace('\t', TAB);
                if open.is_none() {
                    open = Some((doc.lines.len(), new.saturating_sub(1)));
                }
                let tokens = old_hl.line(&text);
                doc.push_line(Kind::Remove, Some(old), None, text, tokens);
                old += 1;
            } else if let Some(text) = raw.strip_prefix('+') {
                let text = text.replace('\t', TAB);
                if open.is_none() {
                    open = Some((doc.lines.len(), new.saturating_sub(1)));
                }
                let tokens = new_hl.line(&text);
                doc.push_line(Kind::Add, None, Some(new), text, tokens);
                new += 1;
            } else if raw.starts_with('\\') {
                // `\ No newline at end of file`
                doc.close_change(&mut open);
                doc.items.push(Item::Note(raw.to_string()));
            } else {
                doc.close_change(&mut open);
                let text = raw.strip_prefix(' ').unwrap_or(raw).replace('\t', TAB);
                // Both sides read an unchanged line, so neither loses its
                // place in a block comment that runs through it.
                let _ = old_hl.line(&text);
                let tokens = new_hl.line(&text);
                doc.push_line(Kind::Context, Some(old), Some(new), text, tokens);
                old += 1;
                new += 1;
            }
        }
        doc.close_change(&mut open);
        Some(doc)
    }

    fn push_line(
        &mut self,
        kind: Kind,
        old: Option<u32>,
        new: Option<u32>,
        text: String,
        tokens: Vec<(TokenKind, String)>,
    ) {
        self.lines.push(Line {
            kind,
            old,
            new,
            text,
            tokens,
            partner: None,
            change: None,
        });
        self.items.push(Item::Line(self.lines.len() - 1));
    }

    /// Close the CHANGE that started at `open`: number it, pair its
    /// removed lines with its added ones.
    fn close_change(&mut self, open: &mut Option<(usize, u32)>) {
        let Some((first, before)) = open.take() else {
            return;
        };
        let end = self.lines.len();
        let index = self.changes.len();
        let removed: Vec<usize> = (first..end)
            .filter(|&i| self.lines[i].kind == Kind::Remove)
            .collect();
        let added: Vec<usize> = (first..end)
            .filter(|&i| self.lines[i].kind == Kind::Add)
            .collect();
        for (&r, &a) in removed.iter().zip(&added) {
            self.lines[r].partner = Some(a);
            self.lines[a].partner = Some(r);
        }
        for line in &mut self.lines[first..end] {
            line.change = Some(index);
        }
        let new_anchor = added
            .first()
            .and_then(|&a| self.lines[a].new)
            .unwrap_or(before);
        self.changes.push(Change {
            hunk: self.hunks.len().saturating_sub(1),
            first,
            end,
            removed: removed.len() as u32,
            added: added.len() as u32,
            new_anchor,
        });
    }
}

/// `@@ -a,b +c,d @@`'s four numbers; a length git leaves out is 1.
pub fn parse_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = line.split_whitespace().skip(1);
    let side = |part: Option<&str>, sign: char| -> Option<(u32, u32)> {
        let part = part?.strip_prefix(sign)?;
        Some(match part.split_once(',') {
            Some((start, len)) => (start.parse().ok()?, len.parse().ok()?),
            None => (part.parse().ok()?, 1),
        })
    };
    let (os, ol) = side(parts.next(), '-')?;
    let (ns, nl) = side(parts.next(), '+')?;
    Some((os, ol, ns, nl))
}

/// One row of a laid-out diff.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// A line about the file, across the pane.
    Note(String, DiffLineKind),
    /// A hunk's header row: `⋯ 42 unchanged lines` above it, and the
    /// section git names after its `@@`.
    Hunk(usize),
    /// One line, in the inline and replacement layouts.
    Line(usize),
    /// A row of the side-by-side layout: the old side's line and the new
    /// side's, either missing where nothing faces the other.
    Pair(Option<usize>, Option<usize>),
    /// The replacement layout's stand-in for removed lines nothing
    /// replaced: `count` of them, from line `first`.
    Removed { first: usize, count: u32 },
}

/// A row and the CHANGE it shows, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidRow {
    pub row: Row,
    pub change: Option<usize>,
}

/// Lay `doc` out in `mode`.
pub fn layout(doc: &DiffDoc, mode: DiffMode) -> Vec<LaidRow> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < doc.items.len() {
        match &doc.items[i] {
            Item::Note(text) => {
                let kind = if text.starts_with('\\') {
                    DiffLineKind::Context
                } else {
                    DiffLineKind::Header
                };
                rows.push(LaidRow {
                    row: Row::Note(text.clone(), kind),
                    change: None,
                });
                i += 1;
            }
            Item::Hunk(h) => {
                if !doc.full {
                    rows.push(LaidRow {
                        row: Row::Hunk(*h),
                        change: None,
                    });
                }
                i += 1;
            }
            Item::Line(l) => {
                let line = &doc.lines[*l];
                let Some(change) = line.change else {
                    let row = match mode {
                        DiffMode::SideBySide => Row::Pair(Some(*l), Some(*l)),
                        _ => Row::Line(*l),
                    };
                    rows.push(LaidRow { row, change: None });
                    i += 1;
                    continue;
                };
                // The whole change at once: its lines are contiguous items.
                let c = &doc.changes[change];
                let removed: Vec<usize> = (c.first..c.end)
                    .filter(|&k| doc.lines[k].kind == Kind::Remove)
                    .collect();
                let added: Vec<usize> = (c.first..c.end)
                    .filter(|&k| doc.lines[k].kind == Kind::Add)
                    .collect();
                let mut push = |row| {
                    rows.push(LaidRow {
                        row,
                        change: Some(change),
                    })
                };
                match mode {
                    DiffMode::SideBySide => {
                        for n in 0..removed.len().max(added.len()) {
                            push(Row::Pair(removed.get(n).copied(), added.get(n).copied()));
                        }
                    }
                    DiffMode::Inline => {
                        for k in c.first..c.end {
                            push(Row::Line(k));
                        }
                    }
                    DiffMode::Replacement => {
                        for &a in &added {
                            push(Row::Line(a));
                        }
                        if removed.len() > added.len() {
                            push(Row::Removed {
                                first: removed[added.len()],
                                count: (removed.len() - added.len()) as u32,
                            });
                        }
                    }
                }
                i += c.end - c.first;
            }
        }
    }
    rows
}

/// The row each CHANGE starts on in `rows`, in change order.
pub fn change_starts(rows: &[LaidRow]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut last = None;
    for (n, row) in rows.iter().enumerate() {
        if row.change.is_some() && row.change != last {
            starts.push(n);
        }
        last = row.change;
    }
    starts
}

/// What a word-level diff splits a line into: a run of word characters,
/// a run of blanks, or one other character apiece — so `foo(bar)` against
/// `foo(baz)` lights `baz` alone.
fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let class = |c: char| {
            if c.is_alphanumeric() || c == '_' {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        };
        let k = class(c);
        let mut end = i + c.len_utf8();
        if k != 2 {
            while let Some(&(j, d)) = chars.peek() {
                if class(d) != k {
                    break;
                }
                end = j + d.len_utf8();
                chars.next();
            }
        }
        out.push(&text[start.max(i)..end]);
        start = end;
    }
    out
}

/// A range of a line's characters, `start..end`.
pub type Span = (usize, usize);

/// The character ranges of `old` and of `new` a word-level diff of the two
/// says changed: what the pane lights over the line's own background.
/// Adjacent ranges are merged.
pub fn word_ranges(old: &str, new: &str) -> (Vec<Span>, Vec<Span>) {
    let (a, b) = (words(old), words(new));
    let offsets = |w: &[&str]| {
        let mut at = vec![0usize];
        for s in w {
            at.push(at.last().unwrap() + s.chars().count());
        }
        at
    };
    let (oa, ob) = (offsets(&a), offsets(&b));
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &a, &b);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let add = |into: &mut Vec<Span>, from: usize, to: usize| {
        if from == to {
            return;
        }
        match into.last_mut() {
            Some(last) if last.1 == from => last.1 = to,
            _ => into.push((from, to)),
        }
    };
    for op in ops {
        match op {
            similar::DiffOp::Equal { .. } => {}
            similar::DiffOp::Delete {
                old_index, old_len, ..
            } => add(&mut left, oa[old_index], oa[old_index + old_len]),
            similar::DiffOp::Insert {
                new_index, new_len, ..
            } => add(&mut right, ob[new_index], ob[new_index + new_len]),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                add(&mut left, oa[old_index], oa[old_index + old_len]);
                add(&mut right, ob[new_index], ob[new_index + new_len]);
            }
        }
    }
    (left, right)
}

/// One `@@` hunk of a raw patch: its ranges, and its bytes — the header
/// line through its last line, line endings as git wrote them, so a CRLF
/// file's patch still applies.
#[derive(Debug, Clone, PartialEq)]
pub struct RawHunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub bytes: Vec<u8>,
}

/// Split a one-file patch into its header (`diff --git` through `+++`)
/// and its hunks.
pub fn split_patch(bytes: &[u8]) -> (Vec<u8>, Vec<RawHunk>) {
    let mut header = Vec::new();
    let mut hunks: Vec<RawHunk> = Vec::new();
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b"@@") {
            let text = String::from_utf8_lossy(line);
            if let Some((os, ol, ns, nl)) = parse_header(text.trim_end()) {
                hunks.push(RawHunk {
                    old_start: os,
                    old_len: ol,
                    new_start: ns,
                    new_len: nl,
                    bytes: line.to_vec(),
                });
                continue;
            }
        }
        match hunks.last_mut() {
            Some(h) => h.bytes.extend_from_slice(line),
            None => header.extend_from_slice(line),
        }
    }
    (header, hunks)
}

/// Does a zero-context hunk of a patch read now cover the CHANGE `c` the
/// pane shows? By the new file's lines, which both diffs number the same
/// way: overlapping ranges when both add lines, the same anchor when both
/// only remove, and an anchor inside the other's range when one side of
/// the pair only removes.
pub fn covers(h: &RawHunk, c: &Change) -> bool {
    let (start, len) = c.new_range();
    match (h.new_len, len) {
        (0, 0) => h.new_start == start,
        (0, _) => (start.saturating_sub(1)..start + len).contains(&h.new_start),
        (_, 0) => (h.new_start.saturating_sub(1)..h.new_start + h.new_len).contains(&start),
        (_, _) => h.new_start < start + len && start < h.new_start + h.new_len,
    }
}

/// The patch of the hunks of `hunks` that cover `c`, under `header`; None
/// when none does.
pub fn patch_for(header: &[u8], hunks: &[RawHunk], c: &Change) -> Option<Vec<u8>> {
    let picked: Vec<&RawHunk> = hunks.iter().filter(|h| covers(h, c)).collect();
    if picked.is_empty() {
        return None;
    }
    let mut out = header.to_vec();
    for h in picked {
        out.extend_from_slice(&h.bytes);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/x.rs b/x.rs\nold mode 100644\nnew mode 100755\nindex 1..2\n--- a/x.rs\n+++ b/x.rs\n\
                        @@ -10,4 +10,5 @@ fn f\n keep\n-gone one\n-gone two\n+new one\n+new two\n+new three\n tail\n\
                        @@ -30,2 +31,1 @@\n same\n-dropped\n";

    fn doc() -> DiffDoc {
        DiffDoc::parse(DIFF, "x.rs", false).unwrap()
    }

    /// Lines are numbered on their sides, the headers git repeats are
    /// dropped, and each run of removed and added lines is one CHANGE, its
    /// n-th removed line paired with its n-th added one.
    #[test]
    fn a_diff_reads_into_hunks_lines_and_changes() {
        let d = doc();
        assert_eq!(d.hunks.len(), 2);
        assert_eq!(d.hunks[0].hidden, 9, "lines 1-9 are not in the diff");
        assert_eq!(d.hunks[1].hidden, 16, "lines 14-29");
        assert_eq!(
            d.items[..2],
            [
                Item::Note("old mode 100644".into()),
                Item::Note("new mode 100755".into())
            ]
        );
        assert_eq!(d.changes.len(), 2);
        let c = &d.changes[0];
        assert_eq!((c.removed, c.added, c.new_anchor), (2, 3, 11));
        let gone = c.first;
        assert_eq!(d.lines[gone].text, "gone one");
        assert_eq!(d.lines[gone].old, Some(11));
        let partner = d.lines[gone].partner.unwrap();
        assert_eq!(d.lines[partner].text, "new one");
        assert_eq!(d.lines[partner].new, Some(11));
        assert_eq!(
            d.lines[c.end - 1].partner,
            None,
            "the third add stands alone"
        );
        let c = &d.changes[1];
        assert_eq!(
            (c.removed, c.added, c.new_anchor),
            (1, 0, 31),
            "after line 31"
        );
        assert_eq!(
            DiffDoc::parse("commit abc\n\n    subject", "x", false),
            None
        );
    }

    /// The three layouts of one diff.
    #[test]
    fn the_three_layouts_place_the_same_lines() {
        let d = doc();
        let rows = |mode| -> Vec<Row> { layout(&d, mode).into_iter().map(|r| r.row).collect() };
        let line = |text: &str| d.lines.iter().position(|l| l.text == text).unwrap();
        let side = rows(DiffMode::SideBySide);
        assert_eq!(side[2], Row::Hunk(0));
        assert_eq!(side[3], Row::Pair(Some(line("keep")), Some(line("keep"))));
        assert_eq!(
            side[4],
            Row::Pair(Some(line("gone one")), Some(line("new one")))
        );
        assert_eq!(side[6], Row::Pair(None, Some(line("new three"))));

        let inline = rows(DiffMode::Inline);
        assert_eq!(inline.len(), 2 + 1 + 7 + 1 + 2, "every line its own row");

        let repl = rows(DiffMode::Replacement);
        assert!(
            !repl.contains(&Row::Line(line("gone one"))),
            "no removed line"
        );
        assert!(repl.contains(&Row::Line(line("new one"))));
        assert_eq!(
            repl.last(),
            Some(&Row::Removed {
                first: line("dropped"),
                count: 1
            })
        );

        let starts = change_starts(&layout(&d, DiffMode::Inline));
        assert_eq!(starts.len(), 2);
        assert_eq!(inline[starts[0]], Row::Line(line("gone one")));
        assert_eq!(inline[starts[1]], Row::Line(line("dropped")));
    }

    /// A full-context diff hides nothing: no hunk header rows.
    #[test]
    fn a_full_context_diff_has_no_hunk_rows() {
        let d = DiffDoc::parse("@@ -1,2 +1,2 @@\n a\n-b\n+c\n", "t.txt", true).unwrap();
        assert!(!layout(&d, DiffMode::Inline)
            .iter()
            .any(|r| matches!(r.row, Row::Hunk(_))));
    }

    /// Words, not characters: the changed identifier is lit whole, the
    /// punctuation around it left alone.
    #[test]
    fn word_ranges_light_whole_words() {
        let (old, new) = word_ranges("let x = foo(bar);", "let x = foo(baz, 1);");
        assert_eq!(old, vec![(12, 15)], "bar");
        assert_eq!(new, vec![(12, 18)], "baz, 1");
        let (old, new) = word_ranges("same", "same");
        assert!(old.is_empty() && new.is_empty());
        let (old, new) = word_ranges("", "added");
        assert_eq!((old, new), (vec![], vec![(0, 5)]));
    }

    /// Each side is highlighted as its own file: a removed keyword is a
    /// keyword, and tabs are expanded before anything is measured.
    #[test]
    fn lines_carry_their_sides_syntax() {
        let d = DiffDoc::parse("@@ -1 +1 @@\n-let\tx = 1;\n+fn x() {}\n", "a.rs", false).unwrap();
        assert_eq!(d.lines[0].text, "let    x = 1;");
        assert_eq!(d.lines[0].tokens[0], (TokenKind::Keyword, "let".into()));
        assert_eq!(d.lines[1].tokens[0], (TokenKind::Keyword, "fn".into()));
    }

    /// A zero-context patch splits into hunks with their bytes intact, CRs
    /// included, and the hunks covering a CHANGE make its patch.
    #[test]
    fn a_change_cuts_its_patch_from_a_zero_context_diff() {
        let raw = b"diff --git a/x b/x\r\nindex 1..2 100644\n--- a/x\n+++ b/x\n\
                    @@ -11,2 +11,3 @@\n-gone one\r\n-gone two\r\n+new one\r\n+new two\r\n+new three\r\n\
                    @@ -31 +31,0 @@\n-dropped\n";
        let (header, hunks) = split_patch(raw);
        assert!(header.starts_with(b"diff --git"));
        assert!(header.ends_with(b"+++ b/x\n"));
        assert_eq!(hunks.len(), 2);
        assert_eq!((hunks[1].new_start, hunks[1].new_len), (31, 0));
        assert!(hunks[0].bytes.ends_with(b"+new three\r\n"));

        let d = doc();
        let first = patch_for(&header, &hunks, &d.changes[0]).unwrap();
        assert!(first.ends_with(b"+new three\r\n"), "only the first hunk");
        let second = patch_for(&header, &hunks, &d.changes[1]).unwrap();
        assert!(second.ends_with(b"-dropped\n"));
        assert!(!String::from_utf8_lossy(&second).contains("new one"));
        let none = Change {
            new_anchor: 90,
            added: 2,
            ..d.changes[0].clone()
        };
        assert_eq!(patch_for(&header, &hunks, &none), None);
    }

    #[test]
    fn modes_round_trip_and_cycle() {
        for m in DiffMode::ALL {
            assert_eq!(DiffMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(DiffMode::parse("unified"), Some(DiffMode::Inline));
        assert_eq!(DiffMode::Replacement.next(), DiffMode::SideBySide);
        assert_eq!(DiffMode::parse("sparkle"), None);
    }
}
