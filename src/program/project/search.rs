//! A search through the files of the project: the matches of a pattern, line by line, in the files
//! the project lists that are text. Places in a line are code points.
//!
//! A pattern is the syntax of the `regex` crate: groups, `|`, repeats, `\d \w \s \b` (the Unicode
//! ones), classes such as `\p{L}`, flags such as `(?i)`; no back-references and no look-ahead or
//! look-behind, which are refused with the words of what is wrong. In return a search takes time
//! that grows with the text and the size of the pattern, never with the number of ways the pattern
//! can match, so a pattern the page sent cannot hold the program.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::fs;
use std::time::Instant;

use regex::{Regex, RegexBuilder};
use serde::Serialize;

use super::paths;
use super::tree::{self, Tree};

/// A file larger than this is not searched.
const FILE_LIMIT: u64 = 2 * 1024 * 1024;

/// A search stops after this many matches.
const MATCH_LIMIT: usize = 2000;

/// A line longer than this many characters is cut around each match.
const LINE_WIDTH: usize = 160;

/// How far before a match the cut of a long line begins, in characters.
const LEAD: usize = 40;

/// How big a pattern may be once it is compiled (the size of the engine's program, in bytes). A
/// search takes time in proportion to it, and a pattern such as `(\w{1000}){1000}` is made to be big.
const SIZE_LIMIT: usize = 1 << 20;

/// How deep groups may nest in a pattern.
const NEST_LIMIT: u32 = 100;

/// How much a search for whole words may read again, as a multiple of the text and a floor. A match
/// that is a part of a word is passed over, and the search goes on from the next place; text made of
/// such matches would read the same stretch again and again. Past this the search says so and stops.
const RETRY_FACTOR: usize = 4;
const RETRY_FLOOR: usize = 1 << 16;

/// What to look for.
pub struct Options {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

/// A match in a line. `line` counts from 1; `start` and `end` are the match in `text`, in code
/// points; `text` is the line, cut around the match when it is long, and `offset` where it begins
/// in the line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub offset: usize,
}

/// The matches in one file.
#[derive(Debug, Clone, Serialize)]
pub struct FileMatches {
    pub path: String,
    pub matches: Vec<Match>,
}

/// What a search found: the files in the order of a listing, how many matches, how many files
/// were read, whether it stopped at its limit, and how long it took.
#[derive(Debug, Serialize)]
pub struct Found {
    pub files: Vec<FileMatches>,
    pub matches: usize,
    pub searched: usize,
    pub truncated: bool,
    pub ms: u64,
}

/// A pattern, compiled.
struct Pattern {
    re: Regex,
    /// Only a match with no letter, digit or `_` beside it counts.
    whole_word: bool,
}

impl Pattern {
    fn new(options: &Options) -> Result<Self, String> {
        let source = if options.regex {
            options.query.clone()
        } else {
            regex::escape(&options.query)
        };
        RegexBuilder::new(&source)
            .case_insensitive(!options.case_sensitive)
            .size_limit(SIZE_LIMIT)
            .nest_limit(NEST_LIMIT)
            .build()
            .map(|re| Self {
                re,
                whole_word: options.whole_word,
            })
            .map_err(|why| format!("Not a regular expression: {}", readable(&why)))
    }

    /// The first match from the byte `from` (a place between characters), as the places it starts
    /// and ends.
    fn find(&self, text: &str, from: usize) -> Result<Option<(usize, usize)>, String> {
        if !self.whole_word {
            return Ok(self
                .re
                .find_at(text, from)
                .map(|found| (found.start(), found.end())));
        }
        // The search goes where a whole word can begin, and a match that is a part of a word is
        // passed over: the next try is from the character after its start.
        let budget = text
            .len()
            .saturating_mul(RETRY_FACTOR)
            .saturating_add(RETRY_FLOOR);
        let mut spent = 0usize;
        let mut at = from;
        loop {
            let Some(place) = next_edge(text, at) else {
                return Ok(None);
            };
            let Some(found) = self.re.find_at(text, place) else {
                return Ok(None);
            };
            if is_whole(text, found.start(), found.end()) {
                return Ok(Some((found.start(), found.end())));
            }
            spent = spent.saturating_add(found.end().saturating_sub(place) + 1);
            if spent > budget {
                return Err(
                    "The search gave up: this text has too many places where the pattern matches a part of a word"
                        .to_owned(),
                );
            }
            let Some(ch) = text
                .get(found.start()..)
                .and_then(|tail| tail.chars().next())
            else {
                return Ok(None);
            };
            at = found.start() + ch.len_utf8();
        }
    }
}

/// What is wrong with a pattern, in a line: the engine's own message has the pattern and a mark
/// under the place in it, over several lines.
fn readable(error: &regex::Error) -> String {
    match error {
        regex::Error::Syntax(text) => {
            let said = text
                .lines()
                .rev()
                .find_map(|line| line.trim().strip_prefix("error:"))
                .map_or(text.as_str(), str::trim);
            if said.contains("backreferences") {
                "this pattern uses a back-reference, which search does not support".to_owned()
            } else if said.contains("look-around") {
                "this pattern uses look-ahead or look-behind, which search does not support"
                    .to_owned()
            } else {
                said.to_owned()
            }
        }
        regex::Error::CompiledTooBig(_) => "the pattern is too big".to_owned(),
        other => other.to_string(),
    }
}

/// Whether a character is part of a word: a letter, a digit or `_`.
fn is_word(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

/// Whether the text from `start` to `end` has no part of a word beside it.
fn is_whole(text: &str, start: usize, end: usize) -> bool {
    let before = text.get(..start).and_then(|head| head.chars().next_back());
    let after = text.get(end..).and_then(|tail| tail.chars().next());
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

/// The first place at or after `at` that has no part of a word before it, none where there is no
/// such place left.
fn next_edge(text: &str, at: usize) -> Option<usize> {
    if !text.get(..at)?.chars().next_back().is_some_and(is_word) {
        return Some(at);
    }
    // The place after a character is one when the character is not part of a word.
    text.get(at..)?
        .char_indices()
        .find(|(_, ch)| !is_word(*ch))
        .map(|(offset, ch)| at + offset + ch.len_utf8())
}

/// The lines of a text, ended by `\n`, `\r\n` or `\r`.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let current = rest?;
        match current.find(['\r', '\n']) {
            None => {
                rest = None;
                Some(current)
            }
            Some(at) => {
                let skip = if current.get(at..at + 2) == Some("\r\n") {
                    2
                } else {
                    1
                };
                rest = current.get(at + skip..);
                current.get(..at)
            }
        }
    })
}

/// The characters of `line` from the `from`th, at most `count` of them.
fn chars_of(line: &str, from: usize, count: usize) -> String {
    line.chars().skip(from).take(count).collect()
}

/// The matches of a file's text, line by line, at most `room` of them.
fn search_text(
    path: &str,
    content: &str,
    pattern: &Pattern,
    mut room: usize,
) -> Result<Option<FileMatches>, String> {
    let mut matches = Vec::new();
    for (index, line) in lines(content).enumerate() {
        if room == 0 {
            break;
        }
        if line.is_empty() {
            continue;
        }
        let mut from = 0;
        let mut width = None;
        while room > 0 && from <= line.len() {
            let Some((first, last)) = pattern.find(line, from)? else {
                break;
            };
            if last <= first {
                // A match of nothing is passed over.
                from = first
                    + line
                        .get(first..)
                        .and_then(|tail| tail.chars().next())
                        .map_or(1, char::len_utf8);
                continue;
            }
            let width = *width.get_or_insert_with(|| line.chars().count());
            let start = line.get(..first).unwrap_or_default().chars().count();
            let stop = start + line.get(first..last).unwrap_or_default().chars().count();
            // A long line is cut around the match, so that the list shows what matched.
            let (offset, text) = if width <= LINE_WIDTH {
                (0, line.to_owned())
            } else {
                let offset = start.saturating_sub(LEAD);
                (offset, chars_of(line, offset, LINE_WIDTH))
            };
            let shown = text.chars().count();
            matches.push(Match {
                line: index + 1,
                start: start - offset,
                end: (stop - offset).min(shown),
                text,
                offset,
            });
            room -= 1;
            from = last;
        }
    }
    Ok((!matches.is_empty()).then(|| FileMatches {
        path: path.to_owned(),
        matches,
    }))
}

/// A file's text, none when it is too large, unreadable or binary. Bytes that are not UTF-8 are
/// replaced, so that the rest is searched.
fn read_text(path: &str) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > FILE_LIMIT {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if tree::is_binary(&bytes) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The matches of `options` in the files of the project `tree`, until `stop` says to stop.
pub fn run(tree: &Tree, options: &Options, stop: &dyn Fn() -> bool) -> Result<Found, String> {
    let started = Instant::now();
    let pattern = Pattern::new(options)?;
    let index = tree.files();
    let mut found = Vec::new();
    let (mut matches, mut searched, mut truncated) = (0, 0, false);
    for path in index.files.iter().filter(|path| !paths::is_absolute(path)) {
        if truncated || stop() {
            break;
        }
        let Some(content) = read_text(&paths::join(&tree.root, path)) else {
            continue;
        };
        searched += 1;
        if let Some(file) = search_text(path, &content, &pattern, MATCH_LIMIT - matches)? {
            matches += file.matches.len();
            truncated = matches >= MATCH_LIMIT;
            found.push(file);
        }
    }
    found.sort_by(|a, b| tree::compare(&a.path, &b.path));
    Ok(Found {
        files: found,
        matches,
        searched,
        truncated,
        ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(query: &str, regex: bool, case_sensitive: bool, whole_word: bool) -> Pattern {
        Pattern::new(&Options {
            query: query.to_owned(),
            regex,
            case_sensitive,
            whole_word,
        })
        .unwrap()
    }

    fn places(found: &FileMatches) -> Vec<(usize, usize, usize, usize)> {
        found
            .matches
            .iter()
            .map(|m| (m.line, m.start, m.end, m.offset))
            .collect()
    }

    #[test]
    fn lines_end_with_any_of_the_three_ends() {
        let all: Vec<&str> = lines("a\r\nb\rc\n\nd").collect();
        assert_eq!(all, ["a", "b", "c", "", "d"]);
    }

    #[test]
    fn places_count_code_points_and_a_long_line_is_cut_around_the_match() {
        let line = format!("{}😀 order {} order", "ş".repeat(5), "x".repeat(300));
        let found = search_text("w", &line, &pattern("order", false, true, false), 10)
            .unwrap()
            .unwrap();
        // Five ş, an emoji and a space: the first match is at 7, and the second far along.
        assert_eq!(places(&found)[0], (1, 7, 12, 0));
        let second = &found.matches[1];
        assert_eq!(second.offset, 314 - LEAD);
        let shown: String = second
            .text
            .chars()
            .skip(second.start)
            .take(second.end - second.start)
            .collect();
        assert_eq!(shown, "order");
        assert!(second.text.chars().count() <= LINE_WIDTH);
    }

    #[test]
    fn a_whole_word_has_no_word_letters_beside_it() {
        let found = search_text(
            "w",
            "order orders (order)_order ördere",
            &pattern("order", false, true, true),
            10,
        )
        .unwrap()
        .unwrap();
        assert_eq!(places(&found), [(1, 0, 5, 0), (1, 14, 19, 0)]);
    }

    #[test]
    fn a_match_of_nothing_is_passed_over_and_the_room_is_kept() {
        let found = search_text("w", "aaa\nbab", &pattern("a*", true, true, false), 2)
            .unwrap()
            .unwrap();
        assert_eq!(places(&found), [(1, 0, 3, 0), (2, 1, 2, 0)]);
    }

    #[test]
    fn a_pattern_that_needs_to_go_back_is_refused_with_words() {
        for (query, words) in [
            ("(a)\\1", "back-reference"),
            ("a(?=b)", "look-ahead"),
            ("(", "Not a regular expression"),
        ] {
            let refused = Pattern::new(&Options {
                query: query.to_owned(),
                regex: true,
                case_sensitive: false,
                whole_word: false,
            });
            let Err(why) = refused else {
                panic!("{query} was taken");
            };
            assert!(why.starts_with("Not a regular expression: "), "{why}");
            assert!(why.contains(words), "{why}");
        }
    }
}
