//! The `compare` and `compare-base` calls: an open file against what the last commit keeps of it, as
//! the blocks the page lays its two columns out by, and the commit's text to show on the left. The
//! program holds the text of the file, so nothing of it is sent; git is asked outside the buffers'
//! lane, so that a slow git holds no edit up.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::Options;
use serde::Serialize;

use super::params::{Checked, Fields, OnePath, Params};
use super::pieces::WIRE_BUDGET;
use super::{Failure, Gone, call};
use crate::project::compare::{self, Block};
use crate::project::diff::TEXT_LIMIT;
use crate::project::git::{Commit, Kept};
use crate::state::buffers::{Slice, piece};
use crate::state::session::Session;

/// The commit's file the page shows on the left.
#[derive(Serialize)]
struct Base {
    id: Option<String>,
    length: usize,
    path: String,
}

/// How the file differs from the last commit, of the text at `version`.
#[derive(Serialize)]
struct Compared {
    version: u64,
    commit: Option<Commit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    why: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<Base>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocks: Option<Vec<Block>>,
}

fn nothing(version: u64, commit: Option<Commit>, why: &'static str) -> Compared {
    Compared {
        version,
        commit,
        why: Some(why),
        base: None,
        blocks: None,
    }
}

fn run(session: &Session, args: OnePath, gone: &Gone<'_>) -> Result<Compared, Failure> {
    let path = args.path;
    let (version, text) = session
        .buffers()
        .for_compare(&path, TEXT_LIMIT)
        .map_err(Failure::from)?;
    // A link is no file git keeps the text of, whatever it points to. A file that is not on disk
    // (yet, or any more) is no link, and is compared all the same.
    let linked = std::fs::symlink_metadata(session.project.absolute(&path))
        .is_ok_and(|meta| meta.file_type().is_symlink());
    if linked || gone() {
        return Ok(nothing(version, None, "outside"));
    }
    let Some((repo, inner)) = session.project.repos.of(&path) else {
        return Ok(nothing(version, None, "outside"));
    };
    let git = &repo.git;
    let kept = git.committed(&inner);
    if gone() {
        return Ok(nothing(version, None, "outside"));
    }
    let commit = git.last_commit();
    let committed = match kept {
        Kept::Outside => return Ok(nothing(version, None, "outside")),
        Kept::Failed(why) => return Err(Failure::Failed(why)),
        Kept::Binary => return Ok(nothing(version, commit, "binary")),
        Kept::TooLarge => return Ok(nothing(version, commit, "too-large")),
        Kept::Text(committed) => committed,
    };
    let Some(text) = text else {
        return Ok(nothing(version, commit, "too-large"));
    };
    Ok(match compare::blocks(&committed.text, &text, WIRE_BUDGET) {
        Ok(blocks) => Compared {
            version,
            commit,
            why: None,
            base: Some(Base {
                id: committed.id,
                length: committed.text.len(),
                path: repo.outer(&committed.path),
            }),
            blocks: Some(blocks),
        },
        Err(why) => nothing(version, commit, why.word()),
    })
}

/// Which piece of the commit's text.
struct BaseOf {
    path: String,
    id: String,
    from: usize,
}

impl Params for BaseOf {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.text("path")?,
            id: fields.text("id")?,
            from: fields.count("from")?,
        })
    }
}

/// The piece of the commit's text of `path` that begins at `from`, if the commit still keeps the
/// blob the page was told of.
fn base(session: &Session, args: BaseOf, _: &Gone<'_>) -> Result<Slice, Failure> {
    let kept = session
        .project
        .repos
        .of(&args.path)
        .map(|(repo, inner)| repo.git.committed(&inner));
    match kept {
        Some(Kept::Text(committed)) if committed.id.as_deref() == Some(args.id.as_str()) => {
            Ok(piece(&committed.text, args.from, WIRE_BUDGET))
        }
        _ => Err(Failure::OutOfSync(format!(
            "The last commit no longer keeps {} as {}",
            args.path, args.id
        ))),
    }
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "compare", run);
    call(options, session, "compare-base", base)
}
