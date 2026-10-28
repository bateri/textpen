//! The calls about the program and the page's own state: `boot` (what the page shows first: the
//! project, its settings, and what is open), `ui` (where the page is: tabs, folders, cursors, kept
//! for the next page and the next run), `settings`, and `handoff` (the pane given to a shell in the
//! project, which gives it back when it ends).
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::sync::Arc;
use std::time::Duration;

use pencere::Options;
use serde::Serialize;
use serde_json::{Value, json};

use super::params::{Checked, Fields, Params};
use super::{Failure, Gone, call};
use crate::project::git::GitInfo;
use crate::project::paths;
use crate::state::buffers::Info;
use crate::state::defaults::{Settings, UiState};
use crate::state::session::Session;
use crate::terminal::handoff::{Asked, Returned};
use crate::terminal::machine;

/// How long `boot` waits for the library to say how the shell ended: the view shown again may ask
/// before the thread that waited for the shell has kept it.
const RETURNED_PATIENCE: Duration = Duration::from_secs(5);

/// Where the command line asked the cursor to go.
#[derive(Serialize)]
struct Focus {
    path: String,
    line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u64>,
}

/// How the shell the pane was lent to ended: its status (null when a signal ended it), how long it
/// ran, and why it could not start.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellEnded {
    exit_code: Option<i32>,
    duration: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl From<Returned> for ShellEnded {
    fn from(returned: Returned) -> Self {
        Self {
            exit_code: returned.status,
            duration: returned.ms,
            error: returned.error,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Boot {
    root: String,
    root_name: String,
    home: String,
    user: String,
    hostname: String,
    remote: bool,
    version: &'static str,
    git: Option<GitInfo>,
    settings: Settings,
    ui: UiState,
    buffers: Vec<Info>,
    handoff: bool,
    close_quits: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    returned: Option<ShellEnded>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focus: Option<Focus>,
    problems: Vec<String>,
}

/// What the page shows first: the project, its settings, and what is open. How the shell the pane
/// was lent to ended, and where the command line put the cursor, are told once.
fn boot(session: &Session, (): (), _: &Gone<'_>) -> Result<Boot, Failure> {
    let root = session.project.root.clone();
    let name = paths::basename(&root);
    Ok(Boot {
        root_name: if name.is_empty() {
            root.clone()
        } else {
            name.to_owned()
        },
        home: machine::home(),
        user: machine::user(),
        hostname: machine::hostname(),
        remote: machine::remote(),
        version: env!("CARGO_PKG_VERSION"),
        git: session.project.git.status(Duration::ZERO),
        settings: session.settings(),
        ui: session.ui(),
        buffers: session.buffers().all(),
        handoff: session.desk.available(),
        close_quits: session.close_quits,
        returned: session
            .desk
            .take_returned(RETURNED_PATIENCE)
            .map(ShellEnded::from),
        focus: session.take_focus().map(|focus| Focus {
            path: focus.path,
            line: focus.line,
            column: focus.column,
        }),
        problems: session.problems.clone(),
        root,
    })
}

struct Ui {
    ui: UiState,
}

impl Params for Ui {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            ui: fields.required("ui")?,
        })
    }
}

/// Where the page is (tabs, folders, cursors), kept for the next page and the next run.
fn ui(session: &Session, args: Ui, _: &Gone<'_>) -> Result<Value, Failure> {
    session.keep(args.ui);
    Ok(json!({}))
}

struct SettingsOf {
    settings: Settings,
}

impl Params for SettingsOf {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            settings: fields.required("settings")?,
        })
    }
}

/// The settings, kept as the program has them (a size made whole and within its bounds).
fn settings(session: &Session, args: SettingsOf, _: &Gone<'_>) -> Result<Value, Failure> {
    Ok(json!({ "settings": session.save_settings(args.settings) }))
}

struct Handoff;

impl Params for Handoff {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        fields.one_of("kind", &["shell"])?;
        Ok(Self)
    }
}

/// The pane lent to a shell in the project, which gives it back when it ends: the answer reaches the
/// page before the view closes. What is not saved is written as drafts first, for a hangup while the
/// shell has the pane.
fn handoff(session: &Session, _: Handoff, _: &Gone<'_>) -> Result<Value, Failure> {
    let asked = session.desk.shell(|| session.buffers().flush_drafts());
    let state = match asked {
        Asked::Scheduled => "scheduled",
        Asked::Busy => "busy",
        Asked::Unavailable => "unavailable",
    };
    Ok(json!({ "state": state }))
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "boot", boot);
    let options = call(options, session, "ui", ui);
    let options = call(options, session, "settings", settings);
    call(options, session, "handoff", handoff)
}
