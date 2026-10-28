//! The state of one textpen program: its project, its open files, what it remembers. The page's own
//! state outlives the page: a page that the terminal starts again finds it here.
//!
//! Plain values in and out: this file knows nothing of the page. What the page is told of it is
//! made from it by api/.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::buffers::{self, Buffers};
use super::defaults::{Settings, UiState};
use super::store::Store;
use crate::project::tree::Tree;
use crate::terminal::handoff::Desk;

/// Where the command line asked the cursor to go: a file, a line (from 1) and a column (from 1, in
/// code points).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Focus {
    pub path: String,
    pub line: u64,
    pub column: Option<u64>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Session {
    pub project: Arc<Tree>,
    /// Who lends the pane to a shell, and takes it back.
    pub desk: Desk,
    buffers: Arc<Mutex<Buffers>>,
    store: Store,
    settings: Mutex<Settings>,
    ui: Mutex<UiState>,
    /// What the command line asked to open; told to the first page only.
    focus: Mutex<Option<Focus>>,
    /// Paths on the command line that could not be opened.
    pub problems: Vec<String>,
    /// The command line named the files, as `$EDITOR` is run: closing the last tab ends textpen.
    pub close_quits: bool,
}

impl Session {
    /// The session of the project at `root` (absolute), which remembers in `store`. When the command
    /// line named `files` (absolute), they are the tabs in place of the last run's, and `focus`
    /// (absolute) is where the cursor goes.
    pub fn new(root: &str, files: &[String], focus: Option<Focus>, store: Store) -> Self {
        let project = Arc::new(Tree::new(root));
        let buffers = buffers::shared(Arc::clone(&project), &store.state);
        let settings = store.settings();
        let mut ui = store.ui(&project.root);
        let close_quits = !files.is_empty();
        let mut told = None;
        if close_quits {
            let name = |path: &str| project.name(&project.absolute(path));
            ui.tabs = files.iter().map(|path| name(path)).collect();
            ui.active = match &focus {
                Some(focus) => Some(name(&focus.path)),
                None => ui.tabs.first().cloned(),
            };
            told = focus.map(|focus| Focus {
                path: name(&focus.path),
                ..focus
            });
        }
        Self {
            desk: Desk::new(&project.root),
            project,
            buffers,
            store,
            settings: Mutex::new(settings),
            ui: Mutex::new(ui),
            focus: Mutex::new(told),
            problems: Vec::new(),
            close_quits,
        }
    }

    /// The open files, while no other call has them.
    pub fn buffers(&self) -> MutexGuard<'_, Buffers> {
        lock(&self.buffers)
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn ui(&self) -> UiState {
        lock(&self.ui).clone()
    }

    /// Where the command line asked the cursor to go, once.
    pub fn take_focus(&self) -> Option<Focus> {
        lock(&self.focus).take()
    }

    /// The page's own state, kept for the next page and the next run.
    pub fn keep(&self, ui: UiState) {
        self.store.save_ui(&self.project.root, &ui);
        *lock(&self.ui) = ui;
    }

    pub fn save_settings(&self, settings: Settings) -> Settings {
        self.store.save_settings(&settings);
        lock(&self.settings).clone_from(&settings);
        settings
    }

    /// The program ends: what is not saved is kept as drafts, and the page's state for next time.
    pub fn ending(&self) {
        self.buffers().flush_drafts();
        self.store.flush();
    }
}
