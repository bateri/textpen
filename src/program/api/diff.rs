//! The `diff` call: where an open file's text differs from what git keeps of it (its index), as the
//! hunks the page marks in its gutter and shows in a peek. The program holds the text, so nothing of
//! it is sent; git is asked outside the buffers' lane, so that a slow git holds no edit up.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::Options;
use serde::Serialize;

use super::params::OnePath;
use super::{Failure, Gone, call};
use crate::project::diff::{self, Hunk};
use crate::state::session::Session;

/// What differs, of the text at `version`.
#[derive(Serialize)]
struct Diffed {
    version: u64,
    /// None where nothing is marked: a file outside the project, a link, one git does not keep or
    /// cannot say of, or one too long to compare.
    hunks: Option<Vec<Hunk>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    more: bool,
}

fn run(session: &Session, args: OnePath, gone: &Gone<'_>) -> Result<Diffed, Failure> {
    let path = args.path;
    let held = session
        .buffers()
        .for_diff(&path, diff::TEXT_LIMIT)
        .map_err(Failure::from)?;
    let Some((version, text)) = held else {
        let version = session
            .buffers()
            .all()
            .into_iter()
            .find(|info| info.path == path)
            .map_or(0, |info| info.version);
        return Ok(none(version));
    };
    // A link is no file git keeps the text of, whatever it points to.
    let linked = std::fs::symlink_metadata(session.project.absolute(&path))
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(true);
    if linked || gone() {
        return Ok(none(version));
    }
    let Some(old) = session.project.git.index_text(&path) else {
        return Ok(none(version));
    };
    if gone() {
        return Ok(none(version));
    }
    Ok(match diff::hunks(&old, &text) {
        Some(found) => Diffed {
            version,
            hunks: Some(found.hunks),
            more: found.more,
        },
        None => none(version),
    })
}

fn none(version: u64) -> Diffed {
    Diffed {
        version,
        hunks: None,
        more: false,
    }
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    call(options, session, "diff", run)
}
