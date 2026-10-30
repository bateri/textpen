//! The calls about the pictures the page draws: `picture` (a picture file, read, measured and given
//! to the terminal to hold, for a tab of the page) and `release` (the tab no longer shows what it
//! showed). What the terminal holds is drawn by the page by a name (`sha256:` and the file's
//! digest); the file's bytes never reach the page.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::Options;
use serde::Serialize;
use serde_json::{Value, json};

use super::params::{Checked, Fields, Params};
use super::{Failure, Gone, call};
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

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "picture", picture);
    call(options, session, "release", release)
}
