//! The `changes` topic: the disk and git, as they change. The page is told one value at once and
//! then one only when something differs from what it was told: the open files that changed on disk,
//! the listed directories that did, git's own state when it changed, and a hand-off of the pane the
//! terminal could not make.
//!
//! It looks every two seconds while it is subscribed, on a thread of its own. Looking is a `stat` of
//! each listed directory and open file and, every few seconds, a `git status`: a watch of the
//! system's would not see git's own state change, nor a disk shared over the network, and would
//! cost a watch for every directory the page ever listed.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use pencere::{Options, Subscription, ViewError};
use serde::Serialize;

use crate::project::git::GitInfo;
use crate::state::buffers::Info;
use crate::state::session::Session;
use crate::terminal::handoff::Failed;

/// How often the topic looks at the disk and at git.
const EVERY: Duration = Duration::from_secs(2);

/// How long what git said is used before it is asked again.
const GIT_AGE: Duration = Duration::from_millis(2500);

#[derive(Serialize)]
struct Changes<'a> {
    /// Left out when it is what the page was told last.
    #[serde(skip_serializing_if = "Option::is_none")]
    git: Option<&'a Option<GitInfo>>,
    buffers: Vec<Info>,
    dirs: Vec<String>,
    #[serde(rename = "handoffFailed", skip_serializing_if = "Option::is_none")]
    handoff_failed: Option<&'static str>,
}

/// What the person is told when the terminal could not lend its pane to a shell.
fn words_of(failed: Failed) -> &'static str {
    match failed {
        Failed::Unsupported => "This terminal cannot hand its pane to a command and back.",
        Failed::Closing => "The view could not be closed just now.",
    }
}

/// Looks until the page stops listening, and tells it what changed.
fn watch(session: &Session, feed: &Subscription) {
    // What the page was told of git: nothing yet.
    let mut told: Option<Option<GitInfo>> = None;
    loop {
        let git = session.project.git.status(GIT_AGE);
        // A buffer whose file changed is read again, or marked as in conflict, as it is told.
        let buffers = session.buffers().changed_on_disk();
        let dirs = session.project.changed_dirs();
        let handoff_failed = session.desk.take_failed().map(words_of);
        let new_git = told.as_ref() != Some(&git);
        if new_git || !buffers.is_empty() || !dirs.is_empty() || handoff_failed.is_some() {
            let changes = Changes {
                git: new_git.then_some(&git),
                buffers,
                dirs,
                handoff_failed,
            };
            if let Ok(value) = serde_json::to_value(&changes) {
                if feed.push(value) {
                    told = Some(git);
                }
            }
        }
        if feed.sleep(EVERY) {
            return;
        }
    }
}

pub fn topic(options: Options, session: &Arc<Session>) -> Options {
    let session = Arc::clone(session);
    options.topic("changes", move |_, subscription| {
        let session = Arc::clone(&session);
        let feed = subscription.clone();
        thread::Builder::new()
            .name("textpen-changes".to_owned())
            .spawn(move || watch(&session, &feed))
            .map(drop)
            .map_err(|why| {
                ViewError::with_message("internal", format!("the changes cannot be watched: {why}"))
            })
    })
}
