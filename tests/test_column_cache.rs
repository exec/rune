//! Differential tests for the `(line, char_index, display_col)` memo shared by
//! `char_idx_to_display_col` and `line_col_to_char_idx`.
//!
//! A stale or wrongly-keyed memo here silently misplaces the cursor rather than
//! crashing, so these compare every cached result against a from-scratch
//! reference computation over long, adversarially-ordered query sequences.

use ropey::Rope;
use rune::editor::{char_display_width, Editor};

/// Reference implementation: always walks from the start of the line.
fn ref_char_idx_to_display_col(rope: &Rope, line: usize, char_offset: usize) -> usize {
    let mut display_col = 0;
    for (i, ch) in rope.line(line).chars().enumerate() {
        if i >= char_offset || ch == '\n' {
            break;
        }
        display_col += char_display_width(ch, display_col);
    }
    display_col
}

/// Reference implementation for the inverse conversion.
fn ref_line_col_to_char_idx(rope: &Rope, line: usize, col: usize) -> usize {
    let line_start = rope.line_to_char(line);
    let mut char_idx = 0;
    let mut display_col = 0;
    for (i, ch) in rope.line(line).chars().enumerate() {
        if display_col >= col || ch == '\n' {
            break;
        }
        let w = char_display_width(ch, display_col);
        if display_col + w > col {
            break;
        }
        char_idx = i + 1;
        display_col += w;
    }
    line_start + char_idx
}

/// Deterministic pseudo-random sequence, so failures reproduce exactly.
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

fn sample_documents() -> Vec<(&'static str, String)> {
    vec![
        (
            "ascii",
            "hello world this is a plain line\nsecond line here\n".to_string(),
        ),
        ("wide", "日本語のテキストです\nmore 日本 text\n".to_string()),
        ("tabs", "\tindented\t\tdeeply\ta\nb\tc\n".to_string()),
        ("mixed", "a\t日x\u{3000}b\tc日\n\tz\n".to_string()),
        ("control", "has\rcontrol\u{7}chars\nnext\n".to_string()),
        (
            "combining",
            "e\u{301}a\u{308}o\u{327}x\nplain\n".to_string(),
        ),
        (
            "emoji",
            "a\u{1f600}b\u{1f1fa}\u{1f1f8}c\nplain\n".to_string(),
        ),
        ("empty_lines", "\n\n\nx\n\n".to_string()),
        ("long", format!("{}\nshort\n", "abc\td日".repeat(400))),
    ]
}

/// The memo must never change an answer, whatever order the queries arrive in.
#[test]
fn cached_conversions_match_reference_under_random_queries() {
    for (name, text) in sample_documents() {
        let rope = Rope::from_str(&text);
        let mut ed = Editor::new_buffer();
        ed.rope = rope.clone();

        let mut seed = 0x5eed_1234_u64;
        for _ in 0..4000 {
            let line = (lcg(&mut seed) as usize) % rope.len_lines();
            let line_chars = rope.line(line).len_chars();
            // Deliberately probe past the end of the line as well.
            let offset = (lcg(&mut seed) as usize) % (line_chars + 3);

            let got = ed.char_idx_to_display_col(line, offset);
            let want = ref_char_idx_to_display_col(&rope, line, offset);
            assert_eq!(
                got, want,
                "[{name}] char_idx_to_display_col(line={line}, offset={offset})"
            );

            let col = (lcg(&mut seed) as usize) % (line_chars * 2 + 5);
            let got = ed.line_col_to_char_idx(line, col);
            let want = ref_line_col_to_char_idx(&rope, line, col);
            assert_eq!(
                got, want,
                "[{name}] line_col_to_char_idx(line={line}, col={col})"
            );
        }
    }
}

/// Sequential forward and backward scans are the real access pattern (arrow
/// keys), and the one the memo is tuned for -- so check it exhaustively.
#[test]
fn cached_conversions_match_reference_on_sequential_scans() {
    for (name, text) in sample_documents() {
        let rope = Rope::from_str(&text);
        let mut ed = Editor::new_buffer();
        ed.rope = rope.clone();

        for line in 0..rope.len_lines() {
            let n = rope.line(line).len_chars();
            for offset in 0..=n {
                assert_eq!(
                    ed.char_idx_to_display_col(line, offset),
                    ref_char_idx_to_display_col(&rope, line, offset),
                    "[{name}] forward scan line={line} offset={offset}"
                );
            }
            for offset in (0..=n).rev() {
                assert_eq!(
                    ed.char_idx_to_display_col(line, offset),
                    ref_char_idx_to_display_col(&rope, line, offset),
                    "[{name}] backward scan line={line} offset={offset}"
                );
            }
            let width = ed.char_idx_to_display_col(line, n);
            for col in (0..=width + 2).rev() {
                assert_eq!(
                    ed.line_col_to_char_idx(line, col),
                    ref_line_col_to_char_idx(&rope, line, col),
                    "[{name}] backward col scan line={line} col={col}"
                );
            }
        }
    }
}

/// The memo must not survive an edit: every char index after the edit point shifts.
#[test]
fn memo_is_invalidated_by_edits() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("aaaaaaaaaa\nbbbb\n");

    // Warm the memo deep into line 0.
    assert_eq!(ed.char_idx_to_display_col(0, 10), 10);

    // Insert at the front, shifting everything.
    ed.viewport.cursor_pos = (0, 0);
    ed.insert_char('日');

    assert_eq!(ed.rope.line(0).to_string(), "日aaaaaaaaaa\n");
    // Stale memo would report 10 here instead of 11 (wide char = 2 columns).
    assert_eq!(
        ed.char_idx_to_display_col(0, 10),
        ref_char_idx_to_display_col(&ed.rope, 0, 10)
    );
    assert_eq!(ed.char_idx_to_display_col(0, 11), 12);
}

/// Interleaving the two conversions must be safe: they share one memo entry.
#[test]
fn interleaved_conversions_stay_consistent() {
    let rope = Rope::from_str("a\td日x\u{3000}yz\tw\n");
    let mut ed = Editor::new_buffer();
    ed.rope = rope.clone();

    let n = rope.line(0).len_chars();
    let mut seed = 0xabcd_ef01_u64;
    for _ in 0..2000 {
        if lcg(&mut seed).is_multiple_of(2) {
            let offset = (lcg(&mut seed) as usize) % (n + 2);
            assert_eq!(
                ed.char_idx_to_display_col(0, offset),
                ref_char_idx_to_display_col(&rope, 0, offset)
            );
        } else {
            let col = (lcg(&mut seed) as usize) % (n * 2 + 4);
            assert_eq!(
                ed.line_col_to_char_idx(0, col),
                ref_line_col_to_char_idx(&rope, 0, col)
            );
        }
    }
}

/// `insert_char` re-seeds the memo with the known post-insert position instead of
/// leaving it cleared. A wrong seed misplaces the cursor silently, so type long
/// mixed-width sequences and check every conversion against the reference.
#[test]
fn typing_reseeds_the_memo_correctly() {
    for start in ["", "prefix\t日x", "\u{3000}", "abc"] {
        for typed in [
            "hello world",
            "日本語テキスト",
            "\ta\tb\t",
            "e\u{301}x\u{308}y", // combining marks: zero width
            "a\u{1f600}b",       // emoji
            "mix\t日a\u{301}z ",
        ] {
            let mut ed = Editor::new_buffer();
            ed.rope = Rope::from_str(&format!("{start}\n"));
            ed.viewport.cursor_pos = (0, ed.char_idx_to_display_col(0, usize::MAX));

            for ch in typed.chars() {
                ed.insert_char(ch);

                // The cursor column must equal the true width of the prefix
                // before it, and both conversions must agree with a fresh walk.
                let n = ed.rope.line(0).len_chars();
                for offset in 0..=n {
                    assert_eq!(
                        ed.char_idx_to_display_col(0, offset),
                        ref_char_idx_to_display_col(&ed.rope, 0, offset),
                        "start={start:?} typed={typed:?} after {ch:?}, offset={offset}"
                    );
                }
                let width = ref_char_idx_to_display_col(&ed.rope, 0, n);
                for col in 0..=width {
                    assert_eq!(
                        ed.line_col_to_char_idx(0, col),
                        ref_line_col_to_char_idx(&ed.rope, 0, col),
                        "start={start:?} typed={typed:?} after {ch:?}, col={col}"
                    );
                }
            }
        }
    }
}

/// Typing then immediately deleting must not leave a seed describing the
/// pre-delete document.
#[test]
fn edit_then_delete_invalidates_the_seed() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("abc\n");
    ed.viewport.cursor_pos = (0, 3);

    ed.insert_char('日');
    assert_eq!(ed.rope.line(0).to_string(), "abc日\n");
    ed.delete_char();
    assert_eq!(ed.rope.line(0).to_string(), "abc\n");

    for offset in 0..=ed.rope.line(0).len_chars() {
        assert_eq!(
            ed.char_idx_to_display_col(0, offset),
            ref_char_idx_to_display_col(&ed.rope, 0, offset),
            "offset={offset}"
        );
    }
    for col in 0..=4 {
        assert_eq!(
            ed.line_col_to_char_idx(0, col),
            ref_line_col_to_char_idx(&ed.rope, 0, col),
            "col={col}"
        );
    }
}

/// Switching lines must not let one line's memo answer for another.
#[test]
fn memo_does_not_leak_across_lines() {
    let rope = Rope::from_str("日本語日本語日本語\nabcdefghi\n\tx\n");
    let mut ed = Editor::new_buffer();
    ed.rope = rope.clone();

    for _ in 0..200 {
        for line in 0..3 {
            for offset in 0..6 {
                assert_eq!(
                    ed.char_idx_to_display_col(line, offset),
                    ref_char_idx_to_display_col(&rope, line, offset),
                    "line={line} offset={offset}"
                );
            }
        }
    }
}
