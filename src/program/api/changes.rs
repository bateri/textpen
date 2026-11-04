//! The `changes` topic: the disk and git, as they change. The page is told one value at once and
//! then one only when something differs from what it was told: the open files that changed on disk,
//! the listed directories that did, what each repository says when it changed, and a hand-off of the
//! pane the terminal could not make.
//!
//! It looks every two seconds while it is subscribed, on a thread of its own. Looking is a `stat` of
//! each listed directory and open file and, every few seconds, a `git status`: a watch of the
//! system's would not see git's own state change, nor a disk shared over the network, and would
//! cost a watch for every directory the page ever listed.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::collections::HashMap;
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

/// What git says of one repository of the project, by paths from the project's root: none where it
/// cannot say.
#[derive(Serialize)]
pub struct Repo {
    pub root: String,
    pub git: Option<GitInfo>,
}

#[derive(Serialize)]
struct Changes<'a> {
    /// The folders of the project's repositories, where they are not what the page was told last.
    #[serde(skip_serializing_if = "Option::is_none")]
    roots: Option<&'a [String]>,
    /// A repository that says something else now; one a message, as each may fill it.
    #[serde(skip_serializing_if = "Option::is_none")]
    repo: Option<Repo>,
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

/// Tells the page; whether it could be told.
fn say(feed: &Subscription, changes: &Changes<'_>) -> bool {
    serde_json::to_value(changes).is_ok_and(|value| feed.push(value))
}

/// Looks until the page stops listening, and tells it what changed.
fn watch(session: &Session, feed: &Subscription) {
    // What the page was told of the repositories: nothing yet.
    let mut told_roots: Option<Vec<String>> = None;
    let mut told: HashMap<String, Option<GitInfo>> = HashMap::new();
    loop {
        let repos = &session.project.repos;
        repos.look_again();
        let roots: Vec<String> = repos.list().iter().map(|repo| repo.root.clone()).collect();
        told.retain(|root, _| roots.contains(root));
        let mut news = repos
            .due(session.ui().active.as_deref())
            .into_iter()
            .map(|repo| (repo.root.clone(), repo.status(GIT_AGE)))
            .filter(|(root, git)| told.get(root) != Some(git))
            .collect::<Vec<_>>()
            .into_iter();
        // A buffer whose file changed is read again, or marked as in conflict, as it is told.
        let buffers = session.buffers().changed_on_disk();
        let dirs = session.project.changed_dirs();
        let handoff_failed = session.desk.take_failed().map(words_of);
        let new_roots = told_roots.as_ref() != Some(&roots);
        let first = news.next();
        if new_roots
            || first.is_some()
            || !buffers.is_empty()
            || !dirs.is_empty()
            || handoff_failed.is_some()
        {
            let changes = Changes {
                roots: new_roots.then_some(roots.as_slice()),
                repo: first.as_ref().map(|(root, git)| Repo {
                    root: root.clone(),
                    git: git.clone(),
                }),
                buffers,
                dirs,
                handoff_failed,
            };
            if say(feed, &changes) {
                if new_roots {
                    told_roots = Some(roots);
                }
                told.extend(first);
            }
        }
        for (root, git) in news {
            let changes = Changes {
                roots: None,
                repo: Some(Repo {
                    root: root.clone(),
                    git: git.clone(),
                }),
                buffers: Vec::new(),
                dirs: Vec::new(),
                handoff_failed: None,
            };
            if say(feed, &changes) {
                told.insert(root, git);
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
