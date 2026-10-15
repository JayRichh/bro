//! The session pane under streamed output: what the grid's pane shows is
//! what the agent printed, however the bytes were cut into frames on the
//! way and however many draws fell between them.

use super::*;
use crate::app::AttachedTerm;

/// A session's ring as ConPTY wrote it for the Windows end-to-end suite's
/// fake agent, 108×45: the console's clear, then the lines its FEATURE
/// script prints (`crates/nebula/tests/e2e_windows`).
fn conpty_ring() -> Vec<u8> {
    let mut ring = b"\x1b[6n\x1b[?9001h\x1b[?1004h\x1b[?25l\x1b[m".to_vec();
    for _ in 0..44 {
        ring.extend_from_slice(b"\x1b[K\r\n");
    }
    ring.extend_from_slice(b"\x1b[K\x1b[H\x1b]0;C:\\kete\\agent.exe\x07\x1b[?25h");
    for line in FEATURE_LINES {
        if *line == "echo ready" {
            ring.extend_from_slice(b"\x1b[?2004h");
        }
        ring.extend_from_slice(line.as_bytes());
        ring.extend_from_slice(b"\r\n");
    }
    ring
}

const FEATURE_LINES: &[&str] = &[
    "fake agent up",
    "working on: build the feature",
    "QUESTION: ship it? (reply and Enter)",
    "got reply: yes ship it",
    "edited src/feature.txt",
    "committed: add the feature",
    "fake agent finished",
    "echo ready",
];

/// The pane's rows as drawn, each trimmed at the end, the cursor block
/// the pane paints read as the blank under it.
fn pane_rows(terminal: &Terminal<TestBackend>, area: ratatui::layout::Rect) -> Vec<String> {
    let buf = terminal.backend().buffer();
    (area.y..area.y + area.height)
        .map(|y| {
            let mut row = String::new();
            for x in area.x..area.x + area.width {
                match buf[(x, y)].symbol() {
                    "█" => row.push(' '),
                    cell => row.push_str(cell),
                }
            }
            row.trim_end().to_string()
        })
        .collect()
}

/// Cut anywhere — inside an escape, inside a line — and drawn between the
/// two halves, the stream lands in the pane exactly as one parse of it
/// reads: every line whole, none missing.
#[test]
fn the_pane_shows_a_stream_whole_wherever_its_frames_were_cut() {
    let ring = conpty_ring();
    let mut app = App::new();
    seed_tree(&mut app);
    let sref = SessionRef::Agent(AgentId("a1".into()));
    app.pane.term = Some(AttachedTerm::new(sref.clone(), 40, 10));
    let mut terminal = Terminal::new(TestBackend::new(220, 50)).unwrap();
    draw_frame(&mut terminal, &mut app).unwrap();
    let area = app.pane.term_area;
    assert!(area.width > 40 && area.height > 10, "{area:?}");

    let mut reference = vt100::Parser::new(area.height, area.width, 0);
    reference.process(&ring);
    let want: Vec<String> = reference
        .screen()
        .rows(0, area.width)
        .map(|r| r.trim_end().to_string())
        .collect();
    for line in FEATURE_LINES {
        assert!(want.iter().any(|r| r == line), "{line:?} in {want:?}");
    }

    for cut in 0..=ring.len() {
        let mut term = AttachedTerm::new(sref.clone(), area.width, area.height);
        term.apply_scrollback(0, &[]);
        term.apply_output(0, &ring[..cut]);
        app.pane.term = Some(term);
        draw_frame(&mut terminal, &mut app).unwrap();
        if let Some(term) = &mut app.pane.term {
            term.apply_output(cut as u64, &ring[cut..]);
        }
        draw_frame(&mut terminal, &mut app).unwrap();
        assert_eq!(app.pane.term_area, area, "the pane held its size");
        let got = pane_rows(&terminal, area);
        assert_eq!(got, want, "cut at byte {cut}");
    }
}
