//! What differs between a file as the last commit keeps it and as the buffer holds it, in the blocks
//! the page lays its two columns out by: where lines were added, where they were taken away, where
//! a line became another, and in that line the words that changed.
//!
//! Plain values in and out: this file knows nothing of the page. Both texts are as textpen holds a
//! buffer's (line ends `\n`, no byte order mark). A block is all the page needs to know: lines of
//! the old text only (`new_count` 0) are red on the left, lines of the new only (`old_count` 0)
//! green on the right, and with lines on both sides they are blue, the side with fewer lines made
//! as tall as the other by a gap after them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use similar::{Algorithm, DiffOp, TextDiff, capture_diff_slices_deadline};

use super::diff::{HUNK_LIMIT, LINE_LIMIT, TEXT_LIMIT};

/// How long the lines may take to compare before the comparison is given up on.
const TIME_LIMIT: Duration = Duration::from_secs(1);
/// How long the words of two lines may take to compare before the line is shown with none.
const WORDS_TIME: Duration = Duration::from_millis(50);
/// The most lines in a run of changed lines (old times new) whose lines are paired by likeness.
/// Past it the run is one block: each pair costs the length of its lines.
const PAIR_AREA: usize = 4_096;
/// The longest line, bytes, that is compared word by word or paired by likeness.
const WORD_LINE: usize = 2_000;
/// How much two lines are alike, 0 to 1, for them to be a pair.
const ALIKE: f32 = 0.5;

/// Places in a line: `[from, to)` in bytes.
type Ranges = Vec<(usize, usize)>;

/// One place the buffer differs from the commit; lines count from 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub old_line: usize,
    pub old_count: usize,
    pub new_line: usize,
    pub new_count: usize,
    /// In a pair of lines, where the old line differs: `[from, to)` in bytes of the line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub old_words: Ranges,
    /// The same, in the new line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub new_words: Ranges,
}

/// Why there are no blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A text, or its number of lines, is past what is compared.
    TooLarge,
    /// More blocks than the page is told of, or than a message holds.
    TooMany,
}

impl Why {
    /// The word the page is told.
    pub fn word(self) -> &'static str {
        match self {
            Self::TooLarge => "too-large",
            Self::TooMany => "too-many",
        }
    }
}

fn line_of(slice: &str) -> &str {
    slice.trim_end_matches('\n')
}

/// What a block takes in a message, bytes.
fn weight(block: &Block) -> usize {
    serde_json::to_string(block).map_or(0, |told| told.len() + 1)
}

/// The blocks that make the old text the new one, in order. The blocks and the words of the lines
/// that are a pair are told within `budget` bytes of a message: the words that do not fit are left
/// out, and blocks that do not fit are [`Why::TooMany`].
pub fn blocks(old: &str, new: &str, budget: usize) -> Result<Vec<Block>, Why> {
    if old.len() > TEXT_LIMIT || new.len() > TEXT_LIMIT {
        return Err(Why::TooLarge);
    }
    if old == new {
        return Ok(Vec::new());
    }
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .timeout(TIME_LIMIT)
        .diff_lines(old, new);
    let (old_lines, new_lines) = (diff.old_slices(), diff.new_slices());
    if old_lines.len() > LINE_LIMIT || new_lines.len() > LINE_LIMIT {
        return Err(Why::TooLarge);
    }
    let changes = diff
        .ops()
        .iter()
        .filter(|op| !matches!(op, DiffOp::Equal { .. }))
        .count();
    if changes > HUNK_LIMIT {
        return Err(Why::TooMany);
    }
    let mut out = Vec::new();
    // The blocks that are a pair of lines, whose words are told if there is room.
    let mut pairs = Vec::new();
    for op in diff.ops() {
        match *op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => out.push(block((old_index, old_len), (new_index, 0))),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => out.push(block((old_index, 0), (new_index, new_len))),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => replaced(
                (old_index, &old_lines[old_index..old_index + old_len]),
                (new_index, &new_lines[new_index..new_index + new_len]),
                &mut out,
                &mut pairs,
            ),
        }
        if out.len() > HUNK_LIMIT {
            return Err(Why::TooMany);
        }
    }
    let mut used: usize = out.iter().map(weight).sum();
    if used > budget {
        return Err(Why::TooMany);
    }
    for at in pairs {
        let (old_words, new_words) = words(
            line_of(old_lines[out[at].old_line]),
            line_of(new_lines[out[at].new_line]),
        );
        let before = weight(&out[at]);
        out[at].old_words = old_words;
        out[at].new_words = new_words;
        used = used - before + weight(&out[at]);
        if used > budget {
            // The words from here on are left out; the blocks are all told.
            out[at].old_words.clear();
            out[at].new_words.clear();
            break;
        }
    }
    Ok(out)
}

fn block(old: (usize, usize), new: (usize, usize)) -> Block {
    Block {
        old_line: old.0,
        old_count: old.1,
        new_line: new.0,
        new_count: new.1,
        old_words: Vec::new(),
        new_words: Vec::new(),
    }
}

/// A run of changed lines: the old lines `old` (from the line `.0`) became the new ones. A line
/// that is alike one of the other side is paired with it, in order, and the lines between two
/// pairs are a block of their own; lines of no likeness are not a pair for the sake of a place.
fn replaced(
    old: (usize, &[&str]),
    new: (usize, &[&str]),
    out: &mut Vec<Block>,
    pairs: &mut Vec<usize>,
) {
    let (old_at, old_lines) = old;
    let (new_at, new_lines) = new;
    let matched = if old_lines.len() == 1 && new_lines.len() == 1 {
        // One line that became one: it was changed, whatever is left of it.
        vec![(0, 0)]
    } else if old_lines.len() * new_lines.len() <= PAIR_AREA {
        matches(old_lines, new_lines)
    } else {
        Vec::new()
    };
    let (mut o, mut n) = (0, 0);
    for (a, b) in matched {
        if a > o || b > n {
            out.push(block((old_at + o, a - o), (new_at + n, b - n)));
        }
        // A pair the same to the last byte is not a change.
        if old_lines[a] != new_lines[b] {
            pairs.push(out.len());
            out.push(block((old_at + a, 1), (new_at + b, 1)));
        }
        (o, n) = (a + 1, b + 1);
    }
    if o < old_lines.len() || n < new_lines.len() {
        out.push(block(
            (old_at + o, old_lines.len() - o),
            (new_at + n, new_lines.len() - n),
        ));
    }
}

/// What is a word, for the likeness of lines and for the words that differ.
#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Letters,
    Space,
    Mark,
}

fn kind_of(ch: char) -> Kind {
    if ch.is_alphanumeric() {
        Kind::Letters
    } else if ch.is_whitespace() {
        Kind::Space
    } else {
        Kind::Mark
    }
}

/// The words of a line, where each begins and what it is: a run of letters and digits, a run of
/// white space, or any other character alone (so that `8080);` is `8080`, `)` and `;`).
fn tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out: Vec<(usize, &str)> = Vec::new();
    let mut last: Option<Kind> = None;
    for (at, ch) in line.char_indices() {
        let kind = kind_of(ch);
        match out.last_mut() {
            Some(word) if last == Some(kind) && kind != Kind::Mark => {
                word.1 = &line[word.0..at + ch.len_utf8()];
            }
            _ => out.push((at, &line[at..at + ch.len_utf8()])),
        }
        last = Some(kind);
    }
    out
}

/// How often each word that is not white space is in a line, and how many there are.
fn bag(line: &str) -> (HashMap<&str, u32>, u32) {
    let mut counts = HashMap::new();
    let mut total = 0;
    for (_, word) in tokens(line) {
        if word.chars().next().is_some_and(char::is_whitespace) {
            continue;
        }
        *counts.entry(word).or_insert(0) += 1;
        total += 1;
    }
    (counts, total)
}

/// How alike two lines are, 0 to 1: the share of their words they have in common.
fn alike(a: &(HashMap<&str, u32>, u32), b: &(HashMap<&str, u32>, u32)) -> f32 {
    if a.1 == 0 && b.1 == 0 {
        return 1.0;
    }
    if a.1 == 0 || b.1 == 0 {
        return 0.0;
    }
    let (small, large) = if a.0.len() <= b.0.len() {
        (&a.0, &b.0)
    } else {
        (&b.0, &a.0)
    };
    let common: u32 = small
        .iter()
        .map(|(word, count)| (*count).min(large.get(word).copied().unwrap_or(0)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let share = 2.0 * common as f32 / (a.1 + b.1) as f32;
    share
}

/// The lines of `old` and of `new` that are a pair, as indexes in the two, in order: the matching
/// of most likeness among pairs that are alike enough.
fn matches(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (old.len(), new.len());
    let long = |line: &str| line.len() > WORD_LINE;
    let bags_old: Vec<_> = old.iter().map(|line| bag(line_of(line))).collect();
    let bags_new: Vec<_> = new.iter().map(|line| bag(line_of(line))).collect();
    // How alike each line of the old text is to each of the new: 0 where they are not alike enough.
    let mut likeness = vec![0.0_f32; n * m];
    for i in 0..n {
        for j in 0..m {
            likeness[i * m + j] = if old[i] == new[j] {
                1.0
            } else if long(old[i]) || long(new[j]) {
                0.0
            } else {
                let share = alike(&bags_old[i], &bags_new[j]);
                if share >= ALIKE { share } else { 0.0 }
            };
        }
    }
    // The most likeness lines 0..i of the old and 0..j of the new can be matched with.
    let row = m + 1;
    let mut best = vec![0.0_f32; (n + 1) * row];
    for i in 1..=n {
        for j in 1..=m {
            let mut here = best[(i - 1) * row + j].max(best[i * row + j - 1]);
            let like = likeness[(i - 1) * m + j - 1];
            if like > 0.0 {
                here = here.max(best[(i - 1) * row + j - 1] + like);
            }
            best[i * row + j] = here;
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let like = likeness[(i - 1) * m + j - 1];
        if like > 0.0 && best[i * row + j] == best[(i - 1) * row + j - 1] + like {
            out.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if best[i * row + j] == best[(i - 1) * row + j] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    out.reverse();
    out
}

/// Where the words of two lines differ: `[from, to)` in bytes of the old line, and of the new.
fn words(old: &str, new: &str) -> (Ranges, Ranges) {
    if old.len() > WORD_LINE || new.len() > WORD_LINE {
        return (Vec::new(), Vec::new());
    }
    let (a, b) = (tokens(old), tokens(new));
    let (a_words, b_words): (Vec<&str>, Vec<&str>) = (
        a.iter().map(|word| word.1).collect(),
        b.iter().map(|word| word.1).collect(),
    );
    let ops = capture_diff_slices_deadline(
        Algorithm::Myers,
        &a_words,
        &b_words,
        Some(Instant::now() + WORDS_TIME),
    );
    let span = |words: &[(usize, &str)], from: usize, len: usize| {
        let last = words[from + len - 1];
        (words[from].0, last.0 + last.1.len())
    };
    let (mut old_words, mut new_words) = (Vec::new(), Vec::new());
    for op in ops {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => old_words.push(span(&a, old_index, old_len)),
            DiffOp::Insert {
                new_index, new_len, ..
            } => new_words.push(span(&b, new_index, new_len)),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                old_words.push(span(&a, old_index, old_len));
                new_words.push(span(&b, new_index, new_len));
            }
        }
    }
    (old_words, new_words)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDGET: usize = 256 * 1024;

    /// The blocks as `(old_line, old_count, new_line, new_count)`.
    fn told(old: &str, new: &str) -> Vec<(usize, usize, usize, usize)> {
        blocks(old, new, BUDGET)
            .expect("compared")
            .iter()
            .map(|b| (b.old_line, b.old_count, b.new_line, b.new_count))
            .collect()
    }

    /// The words of the one pair of lines the texts differ in, as the text they cover.
    fn marked<'a>(old: &'a str, new: &'a str) -> (Vec<&'a str>, Vec<&'a str>) {
        let found = blocks(old, new, BUDGET).expect("compared");
        assert_eq!(found.len(), 1, "{found:?}");
        let b = &found[0];
        assert_eq!((b.old_count, b.new_count), (1, 1));
        let (a, c) = (
            old.lines().nth(b.old_line).unwrap(),
            new.lines().nth(b.new_line).unwrap(),
        );
        (
            b.old_words.iter().map(|w| &a[w.0..w.1]).collect(),
            b.new_words.iter().map(|w| &c[w.0..w.1]).collect(),
        )
    }

    #[test]
    fn the_same_text_has_no_block_and_lines_added_or_taken_away_are_blocks_of_one_side() {
        assert!(told("a\nb\n", "a\nb\n").is_empty());
        // Two lines added after the first; one taken away, with where it stood in the other text.
        assert_eq!(told("a\nz\n", "a\nb\nc\nz\n"), vec![(1, 0, 1, 2)]);
        assert_eq!(told("a\nb\nc\n", "a\nc\n"), vec![(1, 1, 1, 0)]);
        // All of a text added, or all taken away.
        assert_eq!(told("", "a\nb\n"), vec![(0, 0, 0, 2)]);
        assert_eq!(told("a\nb\n", ""), vec![(0, 2, 0, 0)]);
    }

    #[test]
    fn a_line_that_became_another_is_a_pair_with_the_words_that_changed() {
        let (old, new) = marked(
            "const PORT = Number(process.env.PORT ?? 8080);\n",
            "const PORT = Number(process.env.PORT ?? 3000);\n",
        );
        // The number, not `8080);`.
        assert_eq!((old, new), (vec!["8080"], vec!["3000"]));
        let (old, new) = marked("let a = 1;\n", "let a = 1; // one\n");
        assert_eq!((old, new), (vec![], vec![" // one"]));
        // A line that is nothing like the one it became is a pair all the same: it was one line.
        let (old, new) = marked("alpha\n", "omega\n");
        assert_eq!((old, new), (vec!["alpha"], vec!["omega"]));
    }

    #[test]
    fn of_one_line_that_became_three_the_two_that_are_new_are_added_and_the_one_it_became_is_a_pair()
     {
        let old = "a\n  send(res, 200, { order, receipt: receipt(order.items) });\nz\n";
        let new = "a\n  // A receipt only for what was paid.\n  const paid = order.paidAt !== null;\n  send(res, 200, { order, receipt: paid ? receipt(order.items) : null });\nz\n";
        let found = blocks(old, new, BUDGET).expect("compared");
        let shape: Vec<_> = found
            .iter()
            .map(|b| (b.old_line, b.old_count, b.new_line, b.new_count))
            .collect();
        assert_eq!(shape, vec![(1, 0, 1, 2), (1, 1, 3, 1)]);
        assert!(found[0].old_words.is_empty() && found[0].new_words.is_empty());
        let line = new.lines().nth(3).unwrap();
        let said: Vec<&str> = found[1].new_words.iter().map(|w| &line[w.0..w.1]).collect();
        assert_eq!(said, vec!["paid ? ", ": null "]);
        assert!(found[1].old_words.is_empty());
    }

    #[test]
    fn lines_are_paired_by_likeness_in_order_and_the_lines_between_pairs_are_blocks_of_their_own() {
        let old = "alpha one\nunlike\nbeta two\n";
        let new = "alpha 1\nbeta 2\nextra line here\n";
        let found = blocks(old, new, BUDGET).expect("compared");
        let shape: Vec<_> = found
            .iter()
            .map(|b| (b.old_line, b.old_count, b.new_line, b.new_count))
            .collect();
        // `alpha` pair, `unlike` gone, `beta` pair, `extra` added.
        assert_eq!(
            shape,
            vec![(0, 1, 0, 1), (1, 1, 1, 0), (2, 1, 1, 1), (3, 0, 2, 1)]
        );
        // Lines the same to the last byte inside a changed run are not marked.
        assert_eq!(
            told("a\n\n\nb\n", "x\n\n\ny\n"),
            vec![(0, 1, 0, 1), (3, 1, 3, 1)]
        );
    }

    #[test]
    fn a_last_line_with_no_line_end_is_a_pair_with_no_words() {
        let found = blocks("a\nb\n", "a\nb", BUDGET).expect("compared");
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].old_line, found[0].old_count, found[0].new_count),
            (1, 1, 1)
        );
        assert!(found[0].old_words.is_empty() && found[0].new_words.is_empty());
    }

    #[test]
    fn the_words_are_on_the_edges_of_characters() {
        let old = "héllo wörld 😀 ünï\n";
        let new = "héllo wörlt 😁 ünï\n";
        let found = blocks(old, new, BUDGET).expect("compared");
        assert_eq!(found.len(), 1);
        let (a, b) = (old.trim_end(), new.trim_end());
        for w in &found[0].old_words {
            assert!(a.is_char_boundary(w.0) && a.is_char_boundary(w.1), "{w:?}");
        }
        for w in &found[0].new_words {
            assert!(b.is_char_boundary(w.0) && b.is_char_boundary(w.1), "{w:?}");
        }
        let said: Vec<&str> = found[0].old_words.iter().map(|w| &a[w.0..w.1]).collect();
        assert_eq!(said, vec!["wörld", "😀"]);
    }

    #[test]
    fn what_is_past_the_limits_is_not_compared_and_says_why() {
        assert_eq!(
            blocks(&"x\n".repeat(TEXT_LIMIT), "y\n", BUDGET),
            Err(Why::TooLarge)
        );
        assert_eq!(
            blocks(&"x\n".repeat(LINE_LIMIT + 1), "y\n", BUDGET),
            Err(Why::TooLarge)
        );
        let old: String = (0..=HUNK_LIMIT)
            .map(|n| format!("keep {n}\nold {n}\n"))
            .collect();
        let new: String = (0..=HUNK_LIMIT)
            .map(|n| format!("keep {n}\nnew {n}\n"))
            .collect();
        assert_eq!(blocks(&old, &new, BUDGET), Err(Why::TooMany));
        // Blocks that do not fit a message are too many, however few they are.
        assert_eq!(blocks("a\nb\n", "a\nc\n", 20), Err(Why::TooMany));
        assert_eq!(Why::TooMany.word(), "too-many");
    }

    #[test]
    fn when_the_message_is_full_the_words_are_dropped_and_the_blocks_stay() {
        let old: String = (0..200).map(|n| format!("keep {n}\nold {n} x\n")).collect();
        let new: String = (0..200).map(|n| format!("keep {n}\nnew {n} x\n")).collect();
        let whole = blocks(&old, &new, BUDGET).expect("compared");
        assert!(whole.iter().all(|b| !b.old_words.is_empty()));
        let bare: usize = whole
            .iter()
            .map(|b| {
                weight(&Block {
                    old_words: Vec::new(),
                    new_words: Vec::new(),
                    ..b.clone()
                })
            })
            .sum();
        let tight = blocks(&old, &new, bare + 600).expect("compared");
        assert_eq!(tight.len(), whole.len());
        let with: usize = tight.iter().filter(|b| !b.old_words.is_empty()).count();
        assert!(with > 0 && with < whole.len(), "{with} of {}", whole.len());
        // The words that are told are the first ones, and the places of the blocks are the same.
        assert!(tight[..with].iter().all(|b| !b.old_words.is_empty()));
        for (a, b) in tight.iter().zip(&whole) {
            assert_eq!(
                (a.old_line, a.old_count, a.new_line, a.new_count),
                (b.old_line, b.old_count, b.new_line, b.new_count)
            );
        }
    }

    #[test]
    fn a_run_of_changed_lines_too_long_to_pair_is_one_block() {
        let old: String = (0..80).map(|n| format!("old line {n}\n")).collect();
        let new: String = (0..80).map(|n| format!("new line {n}\n")).collect();
        assert_eq!(told(&old, &new), vec![(0, 80, 0, 80)]);
    }
}
