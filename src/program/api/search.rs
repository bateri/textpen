//! The `search` call: the matches of a pattern in the files of the project, as many as fit one
//! message; a search too long for one is cut, and says so (`truncated`). It runs on its own thread,
//! as every call does, and stops when the page gives it up.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;

use pencere::Options;

use super::params::{Checked, Fields, Params};
use super::pieces::fitting;
use super::{Failure, Gone, call};
use crate::project::search::{self, Found};
use crate::state::session::Session;

impl Params for search::Options {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            query: fields.text("query")?,
            regex: fields.optional("regex")?.unwrap_or(false),
            case_sensitive: fields.optional("caseSensitive")?.unwrap_or(false),
            whole_word: fields.optional("wholeWord")?.unwrap_or(false),
        })
    }
}

fn run(session: &Session, options: search::Options, gone: &Gone<'_>) -> Result<Found, Failure> {
    let mut found = search::run(&session.project, &options, gone)?;
    let count = fitting(&found.files, |file| {
        serde_json::to_string(file).map_or(0, |text| text.len())
    });
    if count < found.files.len() {
        found.files.truncate(count);
        found.truncated = true;
    }
    Ok(found)
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    call(options, session, "search", run)
}
