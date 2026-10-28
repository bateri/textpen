//! The calls about the program and the page's own state: `boot` (what the page shows first: the
//! project, its settings, and what is open), `ui` (where the page is: tabs, folders, cursors, kept
//! for the next page and the next run), `settings`, and `handoff` (the pane given to a shell).
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
use crate::terminal::machine;

/// Where the command line asked the cursor to go.
#[derive(Serialize)]
struct Focus {
    path: String,
    line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u64>,
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
    focus: Option<Focus>,
    problems: Vec<String>,
}

/// What the page shows first: the project, its settings, and what is open. Where the command line
/// put the cursor is told once.
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
        // The pane is handed to a shell once pencere can give a view over and take it back.
        handoff: false,
        close_quits: session.close_quits,
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

/// The pane given to a shell in the project: not yet, until pencere can hand a view over and take
/// it back.
fn handoff(_: &Session, _: Handoff, _: &Gone<'_>) -> Result<Value, Failure> {
    Ok(json!({ "state": "unavailable" }))
}

pub fn calls(options: Options, session: &Arc<Session>) -> Options {
    let options = call(options, session, "boot", boot);
    let options = call(options, session, "ui", ui);
    let options = call(options, session, "settings", settings);
    call(options, session, "handoff", handoff)
}
