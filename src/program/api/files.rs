//! The calls about the files of the project: `list` (one directory), `files` (every file, for the
//! quick open), `create`, `rename` and `delete`. Whatever writes to the disk is the page's word, and
//! the person's.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not. What
//! is answered is cut to fit one message; a directory too big for one says so (`truncated`).

use std::sync::Arc;

use pencere::Options;
use serde::Serialize;
use serde_json::{Value, json};

use super::params::{Checked, Fields, OnePath, Params};
use super::pieces::fitting;
use super::{Failure, Gone, call};
use crate::project::tree::{Kind, Listing};
use crate::state::buffers::Info;
use crate::state::session::Session;

struct List {
    dir: String,
}

impl Params for List {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            dir: fields.required("dir")?,
        })
    }
}

/// A directory's entries, cut to fit one message.
fn list(session: &Session, args: List, _: &Gone<'_>) -> Result<Listing, Failure> {
    let mut listing = session.project.list(&args.dir)?;
    let count = fitting(&listing.entries, |entry| {
        entry.name.len() + entry.path.len() + 80
    });
    if count < listing.entries.len() {
        listing.entries.truncate(count);
        listing.truncated = true;
    }
    Ok(listing)
}

struct Files {
    from: Option<usize>,
}

impl Params for Files {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            from: fields.optional_count("from")?,
        })
    }
}

/// The files of the project for the quick open, a message's worth from `from`.
fn files(session: &Session, args: Files, _: &Gone<'_>) -> Result<Value, Failure> {
    let from = args.from.unwrap_or(0);
    let index = session.project.files();
    let rest = index.files.get(from..).unwrap_or_default();
    let count = fitting(rest, |path| path.len() + 4);
    let finish = from + count;
    Ok(json!({
        "files": rest.get(..count).unwrap_or_default(),
        "truncated": index.truncated,
        "next": (finish < index.files.len()).then_some(finish),
    }))
}

struct Create {
    path: String,
    kind: Kind,
}

impl Params for Create {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.text("path")?,
            kind: match fields.one_of("kind", &["file", "dir"])? {
                "dir" => Kind::Dir,
                _ => Kind::File,
            },
        })
    }
}

fn create(session: &Session, args: Create, _: &Gone<'_>) -> Result<Value, Failure> {
    let path = session.project.create(&args.path, args.kind)?;
    Ok(json!({ "path": path }))
}

struct Rename {
    from: String,
    to: String,
}

impl Params for Rename {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            from: fields.text("from")?,
            to: fields.text("to")?,
        })
    }
}

#[derive(Serialize)]
struct Renamed {
    path: String,
    buffers: Vec<Info>,
}

/// A file or directory moved, and the open buffers under it with it.
fn rename(session: &Session, args: Rename, _: &Gone<'_>) -> Result<Renamed, Failure> {
    let renamed = session.project.rename(&args.from, &args.to)?;
    let mut buffers = session.buffers();
    if renamed.moved {
        buffers.moved(&renamed.from, &renamed.to);
    }
    Ok(Renamed {
        path: renamed.to,
        buffers: buffers.all(),
    })
}

fn delete(session: &Session, args: OnePath, _: &Gone<'_>) -> Result<Value, Failure> {
    session.project.remove(&args.path)?;
    Ok(json!({ "path": args.path }))
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "list", list);
    let options = call(options, session, "files", files);
    let options = call(options, session, "create", create);
    let options = call(options, session, "rename", rename);
    call(options, session, "delete", delete)
}
