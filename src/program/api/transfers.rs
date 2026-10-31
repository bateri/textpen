//! The calls about the files that cross the terminal: `place` (whether a name is taken in a folder,
//! and the next free one) and `upload` (a file the person gave the page is received into the project,
//! telling how far it has got). The page holds the person's handles and passes them on; the bytes
//! move between the program and the terminal, and the page never sees them.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::Options;
use serde_json::{Value, json};

use super::params::{Checked, Fields, Params};
use super::{Failure, Gone, call, call_reporting};
use crate::state::session::Session;
use crate::state::transfers::{Incoming, Place, Refusal};

struct Asking {
    dir: String,
    name: String,
    also: Vec<String>,
    dash: bool,
}

impl Params for Asking {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            dir: fields.required("dir")?,
            name: fields.text("name")?,
            also: fields.optional("also")?.unwrap_or_default(),
            dash: fields.optional("dash")?.unwrap_or(false),
        })
    }
}

/// Whether `name` is taken in the folder `dir` (or by one of `also`, the names of uploads under
/// way), and the first free name beside it.
fn place(session: &Session, args: Asking, _: &Gone<'_>) -> Result<Place, Failure> {
    Ok(session.transfers.place(
        &session.project,
        &args.dir,
        &args.name,
        &args.also,
        args.dash,
    )?)
}

struct Upload {
    dir: String,
    name: String,
    handle: String,
    size: u64,
    path: Option<String>,
    replace: bool,
}

impl Params for Upload {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            dir: fields.required("dir")?,
            name: fields.text("name")?,
            handle: fields.text("handle")?,
            size: u64::try_from(fields.count("size")?).unwrap_or(u64::MAX),
            path: fields.optional("path")?,
            replace: fields.optional("replace")?.unwrap_or(false),
        })
    }
}

/// The file the person gave is received into `dir`. Told how far it has got as it goes; given up
/// (the page cancels the call) it deletes what it received. A name that is taken is refused
/// (`exists`) unless the page says to replace it.
fn upload(session: &Session, args: Upload, call: &pencere::Call) -> Result<Value, Failure> {
    let incoming = Incoming {
        dir: &args.dir,
        name: &args.name,
        handle: &args.handle,
        size: args.size,
        path: args.path.as_deref(),
        replace: args.replace,
    };
    let total = args.size as f64;
    let (path, size) = session
        .transfers
        .receive(&session.project, &incoming, &mut |done| {
            call.progress(done as f64, Some(total), None);
            !call.is_cancelled()
        })
        .map_err(|refusal| match refusal {
            Refusal::Exists(path) => Failure::Exists(format!("{path} already exists")),
            Refusal::Failed(words) => Failure::Failed(words),
        })?;
    Ok(json!({ "path": path, "size": size }))
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "place", place);
    call_reporting(options, session, "upload", upload)
}
