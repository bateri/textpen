//! What textpen remembers between runs, where the XDG directories say: the settings
//! (`~/.config/textpen/settings.json`) and, for each project, the files that were open and where
//! (`~/.local/state/textpen/projects/`). Nothing is made on the disk until there is something to
//! keep, and a home that cannot be written to is not an error: textpen works, and forgets.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::fs;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::defaults::{
    LineNumbers, Place, SIDEBAR_WIDTHS, Settings, TAB_SIZES, UiState, Whitespace, clamp,
};
use super::later::Later;
use crate::project::paths;

/// How long the page's state is still before it is written.
const UI_DELAY: Duration = Duration::from_secs(1);

/// How many files opened lately are remembered.
const RECENT_LIMIT: usize = 50;

/// A key for a path that can name a file: the drafts and the projects are kept by it (the start of
/// its SHA-256, as earlier textpens named them).
pub fn key_of(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    let mut key = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        key.push_str(&format!("{byte:02x}"));
    }
    key
}

/// `$variable/textpen`, or `~/fallback/textpen` where it is not set.
fn directory(variable: &str, fallback: &str) -> String {
    let base = match std::env::var(variable) {
        Ok(base) if !base.is_empty() => base,
        _ => paths::join(&std::env::var("HOME").unwrap_or_default(), fallback),
    };
    paths::join(&base, "textpen")
}

/// The JSON in a file, none when there is no file or it is not JSON.
fn read_json(path: &str) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Writes JSON to a file, with the directories it is in.
pub fn write_json(path: &str, value: &impl serde::Serialize) {
    let Ok(mut text) = serde_json::to_string_pretty(value) else {
        return;
    };
    text.push('\n');
    let _ = fs::create_dir_all(paths::dirname(path));
    let _ = fs::write(path, text);
}

fn flag(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn size(value: Option<&Value>, sizes: (u32, u32), fallback: u32) -> u32 {
    value
        .and_then(Value::as_f64)
        .map_or(fallback, |number| clamp(number, sizes))
}

/// Settings from what a file says: what is missing or not understood is the default, and what an
/// older textpen wrote is read as this one has it (`whitespace` was on or off; `vim` and `fontSize`
/// are gone).
pub fn settings_of(read: &Map<String, Value>) -> Settings {
    let defaults = Settings::default();
    let whitespace = match read.get("whitespace") {
        Some(Value::Bool(true)) => Whitespace::All,
        Some(Value::Bool(false)) => Whitespace::None,
        Some(Value::String(name)) => Whitespace::named(name).unwrap_or(defaults.whitespace),
        _ => defaults.whitespace,
    };
    Settings {
        wrap: flag(read.get("wrap"), defaults.wrap),
        line_numbers: read
            .get("lineNumbers")
            .and_then(Value::as_str)
            .and_then(LineNumbers::named)
            .unwrap_or(defaults.line_numbers),
        tab_size: size(read.get("tabSize"), TAB_SIZES, defaults.tab_size),
        insert_spaces: flag(read.get("insertSpaces"), defaults.insert_spaces),
        whitespace,
        indent_guides: flag(read.get("indentGuides"), defaults.indent_guides),
        auto_save: flag(read.get("autoSave"), defaults.auto_save),
        sidebar: flag(read.get("sidebar"), defaults.sidebar),
        sidebar_width: size(
            read.get("sidebarWidth"),
            SIDEBAR_WIDTHS,
            defaults.sidebar_width,
        ),
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn place_of(value: &Value) -> Option<Place> {
    let number = |key: &str| match value.get(key) {
        Some(Value::Number(number)) => Some(number.clone()),
        _ => None,
    };
    Some(Place {
        anchor: number("anchor")?,
        head: number("head")?,
        top: number("top")?,
    })
}

/// The page's state as a file kept it: what is not understood is left out.
fn ui_of(read: &Map<String, Value>) -> UiState {
    let mut recent = strings(read.get("recent"));
    recent.truncate(RECENT_LIMIT);
    UiState {
        tabs: strings(read.get("tabs")),
        active: read
            .get("active")
            .and_then(Value::as_str)
            .map(str::to_owned),
        expanded: strings(read.get("expanded")),
        places: read
            .get("places")
            .and_then(Value::as_object)
            .map(|places| {
                places
                    .iter()
                    .filter_map(|(path, place)| Some((path.clone(), place_of(place)?)))
                    .collect()
            })
            .unwrap_or_default(),
        recent,
        preview: strings(read.get("preview")),
    }
}

/// A project's state, waiting to be written.
struct Pending {
    file: String,
    record: Value,
}

/// The settings and the projects' state, on the disk.
pub struct Store {
    config: String,
    /// Where the drafts and the projects' state are.
    pub state: String,
    pending: Arc<Mutex<Option<Pending>>>,
    later: Later<()>,
}

impl Store {
    /// The store where the XDG directories say.
    pub fn from_env() -> Self {
        Self::at(
            directory("XDG_CONFIG_HOME", ".config"),
            directory("XDG_STATE_HOME", ".local/state"),
        )
    }

    /// The store whose settings are in `config` and whose drafts and projects are in `state`.
    pub fn at(config: String, state: String) -> Self {
        let pending: Arc<Mutex<Option<Pending>>> = Arc::new(Mutex::new(None));
        let writer = Arc::clone(&pending);
        Self {
            config,
            state,
            later: Later::new("textpen-ui", move |()| write_pending(&writer)),
            pending,
        }
    }

    fn settings_file(&self) -> String {
        paths::join(&self.config, "settings.json")
    }

    fn ui_file(&self, root: &str) -> String {
        paths::join(&self.state, &format!("projects/{}.json", key_of(root)))
    }

    /// The settings, as the file has them.
    pub fn settings(&self) -> Settings {
        match read_json(&self.settings_file()) {
            Some(Value::Object(read)) => settings_of(&read),
            _ => Settings::default(),
        }
    }

    pub fn save_settings(&self, settings: &Settings) {
        write_json(&self.settings_file(), settings);
    }

    /// The page's state in the project at `root`, as the last run left it.
    pub fn ui(&self, root: &str) -> UiState {
        match read_json(&self.ui_file(root)) {
            Some(Value::Object(read)) if read.get("root").and_then(Value::as_str) == Some(root) => {
                ui_of(&read)
            }
            _ => UiState::default(),
        }
    }

    /// Keeps the page's state for the next run, a moment after it stops changing.
    pub fn save_ui(&self, root: &str, ui: &UiState) {
        let Ok(Value::Object(mut record)) = serde_json::to_value(ui) else {
            return;
        };
        record.insert("root".to_owned(), Value::String(root.to_owned()));
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(Pending {
            file: self.ui_file(root),
            record: Value::Object(record),
        });
        self.later.after((), UI_DELAY);
    }

    /// Writes what is waiting, now: the program is about to end.
    pub fn flush(&self) {
        self.later.cancel(&());
        write_pending(&self.pending);
    }
}

fn write_pending(pending: &Mutex<Option<Pending>>) {
    let taken = pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(Pending { file, record }) = taken {
        write_json(&file, &record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_key_is_the_start_of_the_sha256_of_the_path() {
        // printf %s /tmp/a.txt | shasum -a 256
        assert_eq!(key_of("/tmp/a.txt"), "bdb93d1f15ddf44eca5a6dd3");
    }

    #[test]
    fn the_settings_of_an_older_textpen_are_read_as_this_one_has_them() {
        let Value::Object(old) = json!({
            "vim": true, "fontSize": 20, "whitespace": true, "tabSize": 99, "wrap": "yes",
            "lineNumbers": "relative", "sidebarWidth": 10,
        }) else {
            unreachable!()
        };
        let read = settings_of(&old);
        assert_eq!(read.whitespace, Whitespace::All);
        assert_eq!(read.tab_size, 8);
        assert_eq!(read.sidebar_width, 160);
        assert!(!read.wrap);
        assert!(read.indent_guides);
        assert_eq!(read.line_numbers, LineNumbers::Relative);
        let Value::Object(none) = json!({ "whitespace": false }) else {
            unreachable!()
        };
        assert_eq!(settings_of(&none).whitespace, Whitespace::None);
        let written = serde_json::to_value(&read).unwrap();
        assert!(written.get("vim").is_none() && written.get("fontSize").is_none());
    }

    #[test]
    fn a_kept_state_is_read_back_without_what_is_not_understood() {
        let Value::Object(read) = json!({
            "tabs": ["a", 1, "b"], "active": 5, "expanded": "no",
            "places": { "a": { "anchor": 1, "head": 2, "top": 0 }, "b": 3 },
            "recent": (0..60).map(|n| n.to_string()).collect::<Vec<_>>(),
        }) else {
            unreachable!()
        };
        let ui = ui_of(&read);
        assert_eq!(ui.tabs, ["a", "b"]);
        assert_eq!(ui.active, None);
        assert!(ui.expanded.is_empty());
        assert_eq!(ui.places.keys().collect::<Vec<_>>(), ["a"]);
        assert_eq!(ui.recent.len(), RECENT_LIMIT);
    }
}
