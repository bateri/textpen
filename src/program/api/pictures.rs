//! The calls about the pictures the page draws: `picture` (a picture file, read, measured and given
//! to the terminal to hold, for a tab of the page) and `release` (the tab no longer shows what it
//! showed). What the terminal holds is drawn by the page by a name (`sha256:` and the file's
//! digest); the file's bytes never reach the page.
//!
//! A PDF the tab shows is also read by the program, which the page asks where the words of a page
//! are (`pdf-text`, the lines with the place of every letter, for selecting and copying) and where
//! a pattern is in the whole of it (`pdf-find`). Neither is a picture: they are text, in the sizes
//! of the pages, as many lines and matches as fit one message.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::Options;
use serde::Serialize;
use serde_json::{Value, json};

use super::params::{Checked, Fields, Params};
use super::pieces::fitting;
use super::{Failure, Gone, call};
use crate::project::pdf::{Document, Line, PageMatch};
use crate::project::search;
use crate::state::pictures::{Picture, Refusal};
use crate::state::session::Session;

struct Show {
    path: String,
    owner: String,
    from: Option<String>,
}

impl Params for Show {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.text("path")?,
            owner: fields.text("owner")?,
            from: fields.optional("from")?,
        })
    }
}

/// A picture the page can draw, or why the file is not shown as one.
#[derive(Serialize)]
#[serde(untagged)]
enum Answer {
    Shown(Picture),
    Refused {
        refused: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
    },
}

/// The picture at `path` (or, with `from`, as the Markdown file `from` wrote it), held by the
/// terminal for `owner`.
fn picture(session: &Session, args: Show, _: &Gone<'_>) -> Result<Answer, Failure> {
    let shown = session.pictures.show(
        &session.project,
        &args.path,
        args.from.as_deref(),
        &args.owner,
    )?;
    Ok(match shown {
        Ok(picture) => Answer::Shown(picture),
        Err(Refusal::TooLarge { size }) => Answer::Refused {
            refused: "too-large",
            size: Some(size),
        },
        Err(Refusal::NotAPicture) => Answer::Refused {
            refused: "not-a-picture",
            size: None,
        },
    })
}

struct Release {
    owner: String,
}

impl Params for Release {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            owner: fields.text("owner")?,
        })
    }
}

/// The pictures held for `owner` are let go, those nobody else holds.
fn release(session: &Session, args: Release, _: &Gone<'_>) -> Result<Value, Failure> {
    session.pictures.release(&args.owner);
    Ok(json!({}))
}

/// The PDF `owner` shows.
fn document(session: &Session, owner: &str) -> Result<Arc<Document>, Failure> {
    session
        .pictures
        .document(owner)
        .ok_or_else(|| Failure::Failed("That PDF is not open".to_owned()))
}

struct PdfText {
    owner: String,
    /// From 1.
    page: usize,
}

impl Params for PdfText {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            owner: fields.text("owner")?,
            page: fields.positive("page")?,
        })
    }
}

/// The text of one page: its lines, from the top down, as many as fit one message.
#[derive(Serialize)]
struct PageText {
    page: usize,
    lines: Vec<Line>,
    /// Lines are left out that did not fit.
    truncated: bool,
}

/// The lines of page `args.page` of the PDF `args.owner` shows, each with its box and the place of
/// every letter in points from the page's top left corner.
fn pdf_text(session: &Session, args: PdfText, _: &Gone<'_>) -> Result<PageText, Failure> {
    let document = document(session, &args.owner)?;
    let text = args
        .page
        .checked_sub(1)
        .and_then(|index| document.text(index))
        .ok_or_else(|| Failure::Failed(format!("This PDF has no page {}", args.page)))?;
    let mut lines = text.lines.clone();
    let count = fitting(&lines, |line| {
        serde_json::to_string(line).map_or(0, |text| text.len())
    });
    let truncated = count < lines.len();
    lines.truncate(count);
    Ok(PageText {
        page: args.page,
        lines,
        truncated,
    })
}

struct PdfFind {
    owner: String,
    options: search::Options,
}

impl Params for PdfFind {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            owner: fields.text("owner")?,
            options: <search::Options as Params>::read(fields)?,
        })
    }
}

/// What a search through a PDF found: the places, in the order of the pages.
#[derive(Serialize)]
struct PdfFound {
    matches: Vec<PageMatch>,
    searched: usize,
    truncated: bool,
    ms: u64,
}

/// The matches of the pattern in the text of the pages of the PDF `args.owner` shows, until the
/// page gives the search up, the matches are 2000 or they no longer fit one message.
fn pdf_find(session: &Session, args: PdfFind, gone: &Gone<'_>) -> Result<PdfFound, Failure> {
    let document = document(session, &args.owner)?;
    let mut found = document.find(&args.options, gone)?;
    let count = fitting(&found.matches, |found| {
        serde_json::to_string(found).map_or(0, |text| text.len())
    });
    if count < found.matches.len() {
        found.matches.truncate(count);
        found.truncated = true;
    }
    Ok(PdfFound {
        matches: found.matches,
        searched: found.searched,
        truncated: found.truncated,
        ms: found.ms,
    })
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "picture", picture);
    let options = call(options, session, "release", release);
    let options = call(options, session, "pdf-text", pdf_text);
    call(options, session, "pdf-find", pdf_find)
}
