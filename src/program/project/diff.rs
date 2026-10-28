//! The lines that differ between a buffer's text and the text git keeps of the file (the one in its
//! index): the hunks the page marks in its gutter, with the lines as they were for the peek.
//!
//! Plain values in and out: this file knows nothing of the page. Both texts are as textpen holds a
//! buffer's (line ends `\n`, no byte order mark), so that a file with CRLF line ends is not one
//! hunk from its first line to its last.

use std::time::Duration;

use serde::Serialize;
use similar::{Algorithm, DiffOp, TextDiff};

/// How long the lines may take to compare before the diff is given up on.
const TIME_LIMIT: Duration = Duration::from_secs(1);
/// The longest text, bytes, either side of the comparison may be.
pub const TEXT_LIMIT: usize = 2 * 1024 * 1024;
/// The most lines either side may have.
pub const LINE_LIMIT: usize = 50_000;
/// The most hunks that are told; what is past them is not marked.
pub const HUNK_LIMIT: usize = 2_000;
/// The most lines of the original one hunk carries, and the most code points of them.
const OLD_LINES: usize = 64;
const OLD_CHARS: usize = 8 * 1024;
/// The most code points of the original all the hunks carry together: what is told fits one message.
const OLD_BUDGET: usize = 192 * 1024;

/// One place the buffer differs from the original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    /// The first line of it in the buffer, from 0.
    pub line: usize,
    /// How many lines of the buffer it is: 0 where lines were only taken away, which were before
    /// `line`.
    pub count: usize,
    /// The lines the original had there, with no line end.
    pub old: Vec<String>,
    /// Some of `old` is not told (too many lines, too long, or the message is full): the hunk is
    /// still marked.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}

/// What differs, and whether there was more of it than is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunks {
    pub hunks: Vec<Hunk>,
    pub more: bool,
}

fn line_of(slice: &str) -> String {
    slice.trim_end_matches('\n').to_owned()
}

/// What differs between `old` (what git keeps) and `new` (the buffer), none where the texts are too
/// long to compare or the comparison took too long.
pub fn hunks(old: &str, new: &str) -> Option<Hunks> {
    hunks_up_to(old, new, HUNK_LIMIT)
}

/// The same, telling at most `most` hunks.
fn hunks_up_to(old: &str, new: &str, most: usize) -> Option<Hunks> {
    if old.len() > TEXT_LIMIT || new.len() > TEXT_LIMIT {
        return None;
    }
    if old == new {
        return Some(Hunks {
            hunks: Vec::new(),
            more: false,
        });
    }
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(TIME_LIMIT)
        .diff_lines(old, new);
    let (old_lines, new_lines) = (diff.old_slices(), diff.new_slices());
    if old_lines.len() > LINE_LIMIT || new_lines.len() > LINE_LIMIT {
        return None;
    }
    let mut out = Vec::new();
    let mut more = false;
    let mut budget = OLD_BUDGET;
    for op in diff.ops() {
        let (new_at, new_len, old_at, old_len) = match *op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (new_index, 0, old_index, old_len),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (new_index, new_len, old_index, 0),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (new_index, new_len, old_index, old_len),
        };
        if out.len() >= most {
            more = true;
            break;
        }
        let mut lines = Vec::new();
        let mut cut = false;
        let mut chars = 0;
        for slice in old_lines.get(old_at..old_at + old_len).unwrap_or_default() {
            let line = line_of(slice);
            let size = line.chars().count();
            if lines.len() >= OLD_LINES || chars + size > OLD_CHARS || size > budget {
                cut = true;
                break;
            }
            chars += size;
            budget -= size;
            lines.push(line);
        }
        out.push(Hunk {
            line: new_at,
            count: new_len,
            old: lines,
            cut,
        });
    }
    Some(Hunks { hunks: out, more })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn told(old: &str, new: &str) -> Vec<(usize, usize, Vec<String>)> {
        hunks(old, new)
            .expect("compared")
            .hunks
            .into_iter()
            .map(|hunk| (hunk.line, hunk.count, hunk.old))
            .collect()
    }

    fn lines(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn the_same_text_has_no_hunks_and_a_change_is_a_hunk_of_the_lines_it_changed() {
        assert!(told("a\nb\n", "a\nb\n").is_empty());
        // One line changed, one added, one taken away.
        assert_eq!(told("a\nb\nc\n", "a\nB\nc\n"), vec![(1, 1, lines(&["b"]))]);
        assert_eq!(told("a\nb\n", "a\nb\nc\nd\n"), vec![(2, 2, lines(&[]))]);
        assert_eq!(told("a\nb\nc\n", "a\nc\n"), vec![(1, 0, lines(&["b"]))]);
        // Lines changed and more of them added: one hunk, more lines new than old.
        assert_eq!(
            told("a\nb\nz\n", "a\nB\nC\nD\nz\n"),
            vec![(1, 3, lines(&["b"]))]
        );
    }

    #[test]
    fn a_last_line_with_no_line_end_differs_from_the_one_with_it() {
        assert_eq!(told("a\nb\n", "a\nb"), vec![(1, 1, lines(&["b"]))]);
    }

    #[test]
    fn what_the_original_carries_is_cut_to_what_a_message_can_hold() {
        let old: String = (0..200).map(|n| format!("old {n}\n")).collect();
        let found = hunks(&old, "new\n").expect("compared");
        assert_eq!(found.hunks.len(), 1);
        let hunk = &found.hunks[0];
        assert_eq!(
            (hunk.line, hunk.count, hunk.old.len(), hunk.cut),
            (0, 1, 64, true)
        );
        // Past the number of hunks that are told, the rest is not, and it is said.
        let old: String = (0..HUNK_LIMIT + 10)
            .map(|n| format!("keep {n}\nold {n}\n"))
            .collect();
        let new: String = (0..HUNK_LIMIT + 10)
            .map(|n| format!("keep {n}\nnew {n}\n"))
            .collect();
        let found = hunks(&old, &new).expect("compared");
        assert_eq!((found.hunks.len(), found.more), (HUNK_LIMIT, true));
        // A text too long to compare is none.
        assert!(hunks(&"x\n".repeat(TEXT_LIMIT), "y\n").is_none());
    }
}
