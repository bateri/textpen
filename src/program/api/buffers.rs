//! The calls about the open files: `open`, `edit`, `slice`, `save`, `buffer` and `discard`, and
//! `quit`, which ends textpen once they are done. The
//! program holds each file's text (state/buffers); what the page is sent of it is cut to fit one
//! message: a text too long for one is announced (`partial`) and read in pieces with `slice`.
//!
//! The buffer calls run in a lane of their own, `buffers`: one at a time, in the order the page sent
//! them, so that the page may send an edit and then the save of the version it makes without
//! waiting for the edit's answer. Other calls go on meanwhile.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::{CloseResult, Options};
use serde::Serialize;
use serde_json::{Value, json};

use super::params::{Checked, Fields, OnePath, Params};
use super::pieces::WIRE_BUDGET;
use super::{Failure, Gone, call_in};
use crate::state::buffers::{Eol, Info, Opened, Opening, Saved, Slice, Unopenable};
use crate::state::session::Session;

/// A text too long for one message: the page reads it with `slice`.
#[derive(Serialize)]
struct Partial {
    length: usize,
}

/// A buffer with its text, or with the word that its text is to be read in pieces.
#[derive(Serialize)]
struct Sent {
    #[serde(flatten)]
    info: Info,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    partial: Option<Partial>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Answer {
    Opened(Sent),
    Unopenable(Unopenable),
}

/// A buffer as it can go to the page in one message: whole, or announced, to be read in pieces.
fn sendable(opened: Opened) -> Sent {
    if opened.text.len() <= WIRE_BUDGET {
        return Sent {
            info: opened.info,
            text: opened.text,
            partial: None,
        };
    }
    Sent {
        info: opened.info,
        partial: Some(Partial {
            length: opened.text.chars().count(),
        }),
        text: String::new(),
    }
}

fn answer(opening: Opening) -> Answer {
    match opening {
        Opening::Opened(opened) => Answer::Opened(sendable(opened)),
        Opening::Unopenable(unopenable) => Answer::Unopenable(unopenable),
    }
}

/// A file, or the buffer of one already open, with its text.
fn open(session: &Session, args: OnePath, _: &Gone<'_>) -> Result<Answer, Failure> {
    Ok(answer(session.buffers().open(&args.path)))
}

struct Edit {
    path: String,
    base: i64,
    edits: Vec<(i64, i64, String)>,
}

impl Params for Edit {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.required("path")?,
            base: fields.required("base")?,
            edits: fields.required("edits")?,
        })
    }
}

/// The page's edits to a buffer, made on a version the program checks.
fn edit(session: &Session, args: Edit, _: &Gone<'_>) -> Result<Info, Failure> {
    Ok(session.buffers().edit(&args.path, args.base, &args.edits)?)
}

struct SliceOf {
    path: String,
    version: i64,
    from: usize,
}

impl Params for SliceOf {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.required("path")?,
            version: fields.required("version")?,
            from: fields.count("from")?,
        })
    }
}

/// The piece of a text too long for one message that begins at `from`, and where the next begins.
fn slice(session: &Session, args: SliceOf, _: &Gone<'_>) -> Result<Slice, Failure> {
    Ok(session
        .buffers()
        .slice(&args.path, args.version, args.from, WIRE_BUDGET)?)
}

struct Save {
    path: String,
    version: i64,
    force: bool,
}

impl Params for Save {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.required("path")?,
            version: fields.required("version")?,
            force: fields.optional("force")?.unwrap_or(false),
        })
    }
}

fn save(session: &Session, args: Save, _: &Gone<'_>) -> Result<Saved, Failure> {
    Ok(session
        .buffers()
        .save(&args.path, args.version, args.force)?)
}

/// What `buffer` does.
enum Action {
    /// The text again.
    Text,
    /// Read again from the disk.
    Revert,
    /// Keep the changes over a file that changed on disk.
    Keep,
    /// Take the draft, or drop it.
    Draft(bool),
    /// Change the line ends.
    Eol(Eol),
    Close,
}

struct BufferAction {
    path: String,
    action: Action,
    discard: bool,
}

impl Params for BufferAction {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        let action = match fields.one_of(
            "action",
            &[
                "text", "revert", "keep", "restore", "drop", "lf", "crlf", "close",
            ],
        )? {
            "text" => Action::Text,
            "revert" => Action::Revert,
            "keep" => Action::Keep,
            "restore" => Action::Draft(true),
            "drop" => Action::Draft(false),
            "lf" => Action::Eol(Eol::Lf),
            "crlf" => Action::Eol(Eol::Crlf),
            _ => Action::Close,
        };
        Ok(Self {
            path: fields.required("path")?,
            action,
            discard: fields.optional("discard")?.unwrap_or(false),
        })
    }
}

/// What `buffer` answers, by its action.
#[derive(Serialize)]
#[serde(untagged)]
enum Done {
    Opened(Sent),
    Reread(Answer),
    Info(Info),
    Closed { path: String },
}

/// Read a buffer again, from the program or from the disk; keep it over a changed file; take or
/// drop a draft; change its line ends; close it.
fn buffer(session: &Session, args: BufferAction, _: &Gone<'_>) -> Result<Done, Failure> {
    let mut buffers = session.buffers();
    let path = args.path.as_str();
    Ok(match args.action {
        Action::Text => Done::Opened(sendable(buffers.text(path)?)),
        Action::Revert => Done::Reread(answer(buffers.revert(path)?)),
        Action::Keep => Done::Info(buffers.keep(path)?),
        Action::Draft(take) => Done::Opened(sendable(buffers.draft(path, take)?)),
        Action::Eol(eol) => Done::Info(buffers.set_eol(path, eol)?),
        Action::Close => {
            buffers.close(path, args.discard)?;
            Done::Closed { path: args.path }
        }
    })
}

/// Quitting without saving: what is not saved is not kept as a draft either.
fn discard(session: &Session, (): (), _: &Gone<'_>) -> Result<Value, Failure> {
    session.buffers().discard_all();
    Ok(json!({}))
}

/// The page asks textpen to end: the view is closed, and `serve` returns (main keeps what is not saved
/// as drafts, unless `discard` dropped it). A kit view has no close of its own (the protocol, Section
/// 10.6): the program ends it. In the lane of the buffers, after what the page sent before it.
fn quit(session: &Session, (): (), _: &Gone<'_>) -> Result<Value, Failure> {
    match session.desk.control().close() {
        // Closed, the answer has no view to go to; with no view to close, there is nothing to do.
        CloseResult::Closed | CloseResult::Unavailable => Ok(json!({})),
        CloseResult::Unsupported => Err(Failure::Failed(
            "This terminal cannot close the editor's view: ⌃C ends textpen, keeping what is not saved."
                .to_owned(),
        )),
        // What a later version of the library says, which this one does not know.
        _ => Ok(json!({})),
    }
}

/// The lane of the buffer calls.
const LANE: &str = "buffers";

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call_in(options, session, LANE, "open", open);
    let options = call_in(options, session, LANE, "edit", edit);
    let options = call_in(options, session, LANE, "slice", slice);
    let options = call_in(options, session, LANE, "save", save);
    let options = call_in(options, session, LANE, "buffer", buffer);
    let options = call_in(options, session, LANE, "discard", discard);
    call_in(options, session, LANE, "quit", quit)
}
