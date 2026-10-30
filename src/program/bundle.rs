//! The page the terminal is offered: the files of `src/view/` and `src/assets/` that `build.rs`
//! carried into the binary, as a view of pencere's kit at `lua` 2.1 (or 2.0, for a terminal that
//! opens no later).
//!
//! 2.1 is what the pictures need: the page draws a file the program attached by the name the
//! terminal gives it (`sha256:` and the digest), which only a view of 2.1 may. pencere's own
//! `App::views` offers the page at 2.0, so the view is made here from the same files, laid out as
//! it lays them out: the scripts of `src/view/` at the top, the pictures under `assets/`.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::Path;

use pencere::{Bundle, BundleError, File, Kind, View};

/// The files of `src/view/` and `src/assets/`, by their path from `src/`, carried in the binary.
const FILES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/pencere-app.rs"));

/// The variable `pencere dev` sets to the project's `src/`: the files are read from there, once,
/// when the view is made.
const DEV: &str = "PENCERE_DEV_APP";

/// The versions of the kit's page API the page is offered at, the one preferred first. 2.1 is 2.0
/// and the files beside the bundle; a page that finds the terminal has none still shows its text.
const VERSIONS: [&str; 2] = ["2.1", "2.0"];

type Entry = (String, Cow<'static, [u8]>);

/// The page as a view. Under `pencere dev` the files of the project are read from its disk, once;
/// where they cannot be, or are no page, the ones in the binary are.
pub fn view() -> View {
    let dev = std::env::var_os(DEV)
        .filter(|dir| !dir.is_empty())
        .and_then(|dir| from_dir(Path::new(&dir)).ok())
        .and_then(|files| page(files).ok());
    match dev {
        Some(view) => view,
        None => match page(embedded()) {
            Ok(view) => view,
            // The files of the binary are held to be a page by the tests.
            Err(why) => panic!("the files of src/view/ are no page: {why}"),
        },
    }
}

fn embedded() -> Vec<Entry> {
    FILES
        .iter()
        .map(|(path, data)| ((*path).to_owned(), Cow::Borrowed(*data)))
        .collect()
}

/// The page of the files `files`, by their path from `src/`.
fn page(files: Vec<Entry>) -> Result<View, BundleError> {
    let mut list = Vec::new();
    for (path, data) in files {
        if let Some(script) = path.strip_prefix("view/") {
            // Only the scripts a page may require are in its bundle.
            if script.ends_with(".lua") || script.ends_with(".luau") {
                list.push(File::new(script, data));
            }
        } else if path.starts_with("assets/") {
            list.push(File::new(path, data));
        }
    }
    Ok(View::new(
        Kind::Lua,
        &VERSIONS,
        Bundle::new("page.lua", list)?,
    ))
}

/// The files under `src/view/` and `src/assets/` of a project, but for the hidden ones.
fn from_dir(src: &Path) -> io::Result<Vec<Entry>> {
    let mut found = Vec::new();
    let mut pending = vec!["view".to_owned(), "assets".to_owned()];
    while let Some(relative) = pending.pop() {
        let entries = match fs::read_dir(src.join(&relative)) {
            Ok(entries) => entries,
            Err(why) if why.kind() == io::ErrorKind::NotFound => continue,
            Err(why) => return Err(why),
        };
        for entry in entries {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let path = format!("{relative}/{name}");
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else {
                let data = fs::read(src.join(&path))?;
                found.push((path, Cow::Owned(data)));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_files_are_a_bundle_whose_entry_is_the_page() {
        assert!(FILES.iter().any(|(path, _)| *path == "view/page.lua"));
        assert!(
            FILES
                .iter()
                .any(|(path, _)| path.starts_with("assets/icons/"))
        );
        assert!(page(embedded()).is_ok());
    }

    #[test]
    fn the_files_of_a_project_are_read_but_for_the_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path();
        for (path, text) in [
            ("view/page.lua", "return {}"),
            ("view/parts/a.lua", ""),
            ("view/.hidden.lua", ""),
            ("assets/x.svg", "<svg/>"),
        ] {
            let file = src.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        let found: Vec<String> = from_dir(src)
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(found, ["assets/x.svg", "view/page.lua", "view/parts/a.lua"]);
    }
}
