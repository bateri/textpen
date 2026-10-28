//! The settings and the state of the page that the program keeps, what is a default and what is
//! empty in them, and the names their choices have.
//!
//! Plain values: this file knows nothing of the page. The fields are named as they are written to
//! the disk, which is what the page is given.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Number;

/// How the editor numbers lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineNumbers {
    On,
    Relative,
    Off,
}

impl LineNumbers {
    pub const NAMES: [&str; 3] = ["on", "relative", "off"];

    pub fn named(name: &str) -> Option<Self> {
        match name {
            "on" => Some(Self::On),
            "relative" => Some(Self::Relative),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// Which spaces the editor shows: the names of the kit's editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Whitespace {
    None,
    Boundary,
    Selection,
    Trailing,
    All,
}

impl Whitespace {
    pub const NAMES: [&str; 5] = ["none", "boundary", "selection", "trailing", "all"];

    pub fn named(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "boundary" => Some(Self::Boundary),
            "selection" => Some(Self::Selection),
            "trailing" => Some(Self::Trailing),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

/// The person's settings, the same in every project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub wrap: bool,
    pub line_numbers: LineNumbers,
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub whitespace: Whitespace,
    pub indent_guides: bool,
    /// Save a file when the editor loses it: another tab, the shell, quitting.
    pub auto_save: bool,
    pub sidebar: bool,
    pub sidebar_width: u32,
    /// Mark in the gutter the lines that differ from what git keeps (VS Code's `scm.diffDecorations`).
    pub diff_decorations: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wrap: false,
            line_numbers: LineNumbers::On,
            tab_size: 2,
            insert_spaces: true,
            whitespace: Whitespace::Selection,
            indent_guides: true,
            auto_save: false,
            sidebar: true,
            sidebar_width: 240,
            diff_decorations: true,
        }
    }
}

/// The sizes a setting may have.
pub const TAB_SIZES: (u32, u32) = (1, 8);
pub const SIDEBAR_WIDTHS: (u32, u32) = (160, 520);

/// A number as a setting has it: whole, and within its sizes.
pub fn clamp(value: f64, (minimum, maximum): (u32, u32)) -> u32 {
    let whole = (value + 0.5).floor();
    if whole <= f64::from(minimum) {
        minimum
    } else if whole >= f64::from(maximum) {
        maximum
    } else {
        // Between two u32, so it is one.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = whole as u32;
        whole
    }
}

/// Where the cursor and the view of a file were: code points, and the 0-based line at the top. The
/// numbers are kept as they were given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    pub anchor: Number,
    pub head: Number,
    pub top: Number,
}

/// Where the page is in a project, kept for the next page and the next run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UiState {
    pub tabs: Vec<String>,
    pub active: Option<String>,
    pub expanded: Vec<String>,
    pub places: BTreeMap<String, Place>,
    /// Opened lately, the latest first.
    pub recent: Vec<String>,
    /// The files shown as their Markdown preview.
    pub preview: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_made_whole_and_kept_within_its_sizes() {
        assert_eq!(clamp(0.0, TAB_SIZES), 1);
        assert_eq!(clamp(3.4, TAB_SIZES), 3);
        assert_eq!(clamp(3.5, TAB_SIZES), 4);
        assert_eq!(clamp(99.0, TAB_SIZES), 8);
        assert_eq!(clamp(-1e300, SIDEBAR_WIDTHS), 160);
        assert_eq!(clamp(300.2, SIDEBAR_WIDTHS), 300);
    }
}
