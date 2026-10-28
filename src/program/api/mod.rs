//! Everything the page talks to the program about: the calls and the topic of `docs/calls.md`. It
//! knows the page; `project/`, `state/` and `terminal/` do not, and what the page is told is made
//! here from what they hold.
//!
//! A call's params are read before it runs (what does not fit is `bad-params`, and the work never
//! runs), and what the work could not do is told with a code the page acts on: `out-of-sync` for a
//! buffer the program does not hold as the page thinks, `failed` for what the disk or the project
//! would not do (the words are for the person). A call that panics is `internal`, by pencere.

mod buffers;
mod changes;
mod diff;
mod files;
mod params;
mod pieces;
mod search;
mod session;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use pencere::{Call, Options, ViewError};
use serde::Serialize;
use serde_json::Value;

use crate::state::buffers::Error as BufferError;
use crate::state::session::Session;
use params::Params;

/// What a call could not do.
enum Failure {
    /// The disk or the project would not: words for the person.
    Failed(String),
    /// A buffer is not open, or not at the version the page named.
    OutOfSync(String),
}

impl From<String> for Failure {
    fn from(words: String) -> Self {
        Self::Failed(words)
    }
}

impl From<BufferError> for Failure {
    fn from(error: BufferError) -> Self {
        match error {
            BufferError::OutOfSync(words) => Self::OutOfSync(words),
            BufferError::Failed(words) => Self::Failed(words),
        }
    }
}

impl From<Failure> for ViewError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Failed(words) => Self::with_message("failed", words),
            Failure::OutOfSync(words) => Self::with_message("out-of-sync", words),
        }
    }
}

/// Whether the page gave the call up, or is gone: work that takes a while stops then.
type Gone<'a> = dyn Fn() -> bool + 'a;

/// The work of a call: the session, the params it read, and whether the page gave it up.
type Work<P, T> = fn(&Session, P, &Gone<'_>) -> Result<T, Failure>;

/// The handler of a call: its params read as `P`, then `work`.
fn handler<P: Params + 'static, T: Serialize + 'static>(
    session: &Arc<Session>,
    work: Work<P, T>,
) -> impl Fn(Value, &Call) -> Result<T, ViewError> + Send + Sync + 'static {
    let session = Arc::clone(session);
    move |params, call| {
        let params = params::parse::<P>(&params)
            .map_err(|why| ViewError::with_message("bad-params", why))?;
        work(&session, params, &|| call.is_cancelled()).map_err(ViewError::from)
    }
}

/// Answers the calls of the page to `name` with `work`, given the params as `P` reads them. Calls
/// run at the same time as each other.
fn call<P: Params + 'static, T: Serialize + 'static>(
    options: Options,
    session: &Arc<Session>,
    name: &'static str,
    work: Work<P, T>,
) -> Options {
    options.call(name, handler(session, work))
}

/// The same, for a call in `lane`: the calls of one lane run one at a time, in the order the page
/// sent them.
fn call_in<P: Params + 'static, T: Serialize + 'static>(
    options: Options,
    session: &Arc<Session>,
    lane: &'static str,
    name: &'static str,
    work: Work<P, T>,
) -> Options {
    options.call_in(lane, name, handler(session, work))
}

/// The calls and the topic the page may use, answered from `session`.
pub fn serve(options: Options, session: &Arc<Session>) -> Options {
    let options = files::calls(options, session);
    let options = buffers::calls(options, session);
    let options = search::calls(options, session);
    let options = diff::calls(options, session);
    let options = session::calls(options, session);
    changes::topic(options, session)
}
