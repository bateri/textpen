//! The project on disk: the directory textpen was started in (or on), the files in it, and what is
//! made of them (a listing, the index of every file for the quick open, a file created, renamed or
//! removed). Paths travel relative to the root and `/`-separated; a file outside the root travels as
//! its absolute path. What git says of the files is `git`, a search through them `search`.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::git::Git;
use super::paths;
use super::said;
use super::watch::Listed;

/// Directories nobody means to browse or search: version control and installed packages.
const SKIPPED: [&str; 14] = [
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".next",
    ".nuxt",
    ".turbo",
    ".cache",
    "target",
];

/// The most entries a listing has.
const LIST_LIMIT: usize = 5000;

/// The most files the index of the project has.
const INDEX_LIMIT: usize = 50_000;

/// How long the index of the project is used before it is made again.
const INDEX_AGE: Duration = Duration::from_secs(15);

/// A file this large is not searched, and not opened for editing.
pub const TEXT_LIMIT: u64 = 8 * 1024 * 1024;

/// A NUL in the first 8 KiB makes a file binary, as git and grep decide it.
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

/// The order names are listed in, in the tree and in the search: numbers as numbers (`file9` before
/// `file10`), small and capital letters together, a mark or a space before a digit and a digit
/// before a letter; a tie is broken by the bytes, so that the order is the same on every run.
/// Letters with accents are not folded (`é` is after `z`).
pub fn compare(a: &str, b: &str) -> Ordering {
    natural(a, b).then_with(|| a.cmp(b))
}

/// One thing a name is made of, for the order: a number, or a character.
enum Piece<'a> {
    Number(&'a str),
    Mark(char),
}

fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut found = Vec::new();
    let mut digits_from: Option<usize> = None;
    for (at, ch) in text.char_indices() {
        if ch.is_ascii_digit() {
            digits_from.get_or_insert(at);
            continue;
        }
        if let Some(from) = digits_from.take() {
            found.push(Piece::Number(text.get(from..at).unwrap_or_default()));
        }
        found.extend(ch.to_lowercase().map(Piece::Mark));
    }
    if let Some(from) = digits_from {
        found.push(Piece::Number(text.get(from..).unwrap_or_default()));
    }
    found
}

/// Marks and spaces, then numbers, then letters.
fn class(piece: &Piece<'_>) -> u8 {
    match piece {
        Piece::Mark(ch) if !ch.is_alphabetic() => 0,
        Piece::Number(_) => 1,
        Piece::Mark(_) => 2,
    }
}

fn natural(a: &str, b: &str) -> Ordering {
    let (left, right) = (pieces(a), pieces(b));
    for (x, y) in left.iter().zip(&right) {
        let order = class(x).cmp(&class(y)).then_with(|| match (x, y) {
            (Piece::Number(m), Piece::Number(n)) => {
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                m.len().cmp(&n.len()).then_with(|| m.cmp(n))
            }
            (Piece::Mark(m), Piece::Mark(n)) => m.cmp(n),
            _ => Ordering::Equal,
        });
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

/// When the file or directory at `path` last changed (a link is followed), none when it is not
/// there.
pub fn modified(path: &str) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Dir,
}

/// An entry of a listing. A link is followed: `kind` is what it points to.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub link: bool,
    pub size: u64,
}

/// A directory's entries, folders first, in the order of [`compare`].
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub dir: String,
    pub entries: Vec<Entry>,
    pub truncated: bool,
}

/// Every file of the project, by the name it travels by: one reading of them, shared by whoever
/// holds it. `generation` tells the readings apart (each is one more than the one before).
#[derive(Debug, Clone)]
pub struct Index {
    pub files: Arc<Vec<String>>,
    pub truncated: bool,
    pub generation: u64,
}

/// The index, and when it was made.
struct Made {
    at: Instant,
    forgotten: u64,
    index: Index,
}

/// A file or directory moved: the names it had and has, and whether it was moved at all.
pub struct Renamed {
    pub from: String,
    pub to: String,
    pub moved: bool,
}

/// The project.
pub struct Tree {
    /// Absolute, worked out, as the system names the directory the program is in.
    pub root: String,
    pub git: Git,
    /// Directories that were listed, and whether they changed since.
    listed: Mutex<Listed>,
    /// Held while the index is made, so that the callers who ask at once wait for one.
    index: Mutex<Option<Made>>,
    /// The index the last listing read a page at a time began on: its later pages are of it.
    paged: Mutex<Option<Index>>,
    /// How many times what was learnt of the files was forgotten.
    forgotten: AtomicU64,
}

impl Tree {
    /// The project at `root`, an absolute path.
    pub fn new(root: &str) -> Self {
        let root = paths::normalize(root);
        Self {
            git: Git::new(&root),
            root,
            listed: Mutex::new(Listed::default()),
            index: Mutex::new(None),
            paged: Mutex::new(None),
            forgotten: AtomicU64::new(0),
        }
    }

    /// The absolute path of a path the page or the command line gave.
    pub fn absolute(&self, path: &str) -> String {
        paths::resolve(&self.root, path)
    }

    /// How a path travels: relative to the root inside it, absolute outside.
    pub fn name(&self, absolute: &str) -> String {
        let relative = paths::relative(&self.root, absolute);
        // A name that begins with two dots (`..hidden`) is in the project; `..` itself is not.
        if relative == ".." || relative.starts_with("../") || paths::is_absolute(&relative) {
            absolute.to_owned()
        } else {
            relative
        }
    }

    /// The entries of a directory, folders first; cut at five thousand.
    pub fn list(&self, dir: &str) -> Result<Listing, String> {
        let absolute = self.absolute(dir);
        let fail = |why: io::Error| format!("{dir}: {}", said(&why));
        let mut entries = Vec::new();
        for item in fs::read_dir(&absolute).map_err(fail)? {
            let item = item.map_err(fail)?;
            // A name that is not UTF-8 cannot travel to the page.
            let Ok(name) = item.file_name().into_string() else {
                continue;
            };
            let full = paths::join(&absolute, &name);
            let link = item.file_type().is_ok_and(|kind| kind.is_symlink());
            let meta = fs::metadata(&full).or_else(|_| item.metadata()).ok();
            let is_dir = meta.as_ref().is_some_and(fs::Metadata::is_dir);
            let is_other = meta
                .as_ref()
                .is_none_or(|meta| !meta.is_dir() && !meta.is_file());
            let size = match &meta {
                // A link to nothing is of no size.
                _ if link && is_other => 0,
                _ if is_dir && !link => 0,
                Some(meta) => meta.len(),
                None => 0,
            };
            entries.push(Entry {
                path: self.name(&full),
                name,
                kind: if is_dir { Kind::Dir } else { Kind::File },
                link,
                size,
            });
        }
        entries.sort_by(|a, b| {
            (b.kind == Kind::Dir)
                .cmp(&(a.kind == Kind::Dir))
                .then_with(|| compare(&a.name, &b.name))
        });
        let truncated = entries.len() > LIST_LIMIT;
        entries.truncate(LIST_LIMIT);
        let name = self.name(&absolute);
        self.listed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .note(name.clone(), modified(&absolute));
        Ok(Listing {
            dir: name,
            entries,
            truncated,
        })
    }

    /// The listed directories whose entries changed since they were listed (or that went).
    pub fn changed_dirs(&self) -> Vec<String> {
        let changed = self
            .listed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .changed(|dir| modified(&self.absolute(dir)));
        if !changed.is_empty() {
            self.forgotten.fetch_add(1, AtomicOrdering::SeqCst);
        }
        changed
    }

    /// Forgets what was learnt of the files: after the page created, renamed, removed or saved one.
    pub fn forget(&self) {
        self.forgotten.fetch_add(1, AtomicOrdering::SeqCst);
        self.git.forget();
    }

    /// Every file of the project, for the quick open: git's list in a repository, a walk elsewhere.
    pub fn files(&self) -> Index {
        let mut made = self.index.lock().unwrap_or_else(PoisonError::into_inner);
        let forgotten = self.forgotten.load(AtomicOrdering::SeqCst);
        if let Some(known) = made.as_ref() {
            if known.forgotten == forgotten && known.at.elapsed() < INDEX_AGE {
                return known.index.clone();
            }
        }
        let (files, truncated) = match self.git.files() {
            Some(mut files) => {
                let truncated = files.len() > INDEX_LIMIT;
                files.truncate(INDEX_LIMIT);
                (files, truncated)
            }
            None => self.walk(),
        };
        let index = Index {
            files: Arc::new(files),
            truncated,
            generation: made.as_ref().map_or(1, |known| known.index.generation + 1),
        };
        *made = Some(Made {
            at: Instant::now(),
            forgotten,
            index: index.clone(),
        });
        index
    }

    /// The index for a listing read a page at a time, as [`Tree::files`] has it now; kept, so that
    /// the listing's later pages are of the same files however they change meanwhile.
    pub fn files_to_page(&self) -> Index {
        let index = self.files();
        *self.paged.lock().unwrap_or_else(PoisonError::into_inner) = Some(index.clone());
        index
    }

    /// The index a listing began on, for its later pages: the one of `generation`, if that is the
    /// last listing begun.
    pub fn paged_files(&self, generation: u64) -> Option<Index> {
        self.paged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|index| index.generation == generation)
            .cloned()
    }

    /// The files under the root, a level at a time; a link is a file and is not followed. Whether
    /// there were more than the index holds.
    fn walk(&self) -> (Vec<String>, bool) {
        let mut files = Vec::new();
        let mut queue = VecDeque::from([self.root.clone()]);
        while let Some(dir) = queue.pop_front() {
            if files.len() >= INDEX_LIMIT {
                break;
            }
            let Ok(items) = fs::read_dir(&dir) else {
                continue;
            };
            let mut items: Vec<(String, fs::FileType)> = items
                .flatten()
                .filter_map(|item| {
                    Some((item.file_name().into_string().ok()?, item.file_type().ok()?))
                })
                .collect();
            items.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, kind) in items {
                let full = paths::join(&dir, &name);
                if kind.is_symlink() || kind.is_file() {
                    files.push(self.name(&full));
                } else if kind.is_dir() && !SKIPPED.contains(&name.as_str()) {
                    queue.push_back(full);
                }
            }
        }
        let truncated = files.len() >= INDEX_LIMIT;
        files.truncate(INDEX_LIMIT);
        (files, truncated)
    }

    /// Whether `absolute` is the project or a directory it is in, whatever links lie on the way: the
    /// path is worked out by its text, but a link in a directory of it (`up -> ..`) leads elsewhere
    /// when the disk is asked, so the directory it is in is looked at as the disk has it. The last
    /// part is not followed: a link is moved or removed, not what it points to.
    fn holds_project(&self, absolute: &str) -> bool {
        if paths::is_within(&self.root, absolute) {
            return true;
        }
        let real = |path: &str| {
            fs::canonicalize(path)
                .ok()
                .and_then(|real| real.to_str().map(paths::normalize))
        };
        let (Some(root), Some(parent)) = (real(&self.root), real(&paths::dirname(absolute))) else {
            return false;
        };
        paths::is_within(&root, &paths::join(&parent, paths::basename(absolute)))
    }

    /// A file or a directory made where nothing is (not even a link to nothing). The name it has in
    /// the project.
    pub fn create(&self, path: &str, kind: Kind) -> Result<String, String> {
        let absolute = self.absolute(path);
        let taken = || format!("{path} already exists");
        if fs::symlink_metadata(&absolute).is_ok() {
            return Err(taken());
        }
        let fail = |why: io::Error| {
            if why.kind() == io::ErrorKind::AlreadyExists {
                taken()
            } else {
                format!("{path} could not be made: {}", said(&why))
            }
        };
        fs::create_dir_all(paths::dirname(&absolute)).map_err(fail)?;
        match kind {
            Kind::Dir => fs::create_dir(&absolute).map_err(fail)?,
            // Made with no other file there: an existing file is not truncated, and a link that
            // points at nothing is not written through to make its target.
            Kind::File => {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&absolute)
                    .map_err(fail)?;
            }
        }
        self.forget();
        Ok(self.name(&absolute))
    }

    /// A file or a directory moved, where nothing else is. A change of case alone is allowed: on a
    /// disk that ignores case the new name is the same file.
    pub fn rename(&self, from: &str, to: &str) -> Result<Renamed, String> {
        let source = self.absolute(from);
        let target = self.absolute(to);
        if self.holds_project(&source) {
            return Err("The project itself is not moved".to_owned());
        }
        if source != target && paths::is_within(&target, &source) {
            return Err(format!("{from} cannot be moved into itself"));
        }
        if source == target {
            return Ok(Renamed {
                from: self.name(&source),
                to: self.name(&target),
                moved: false,
            });
        }
        if let Ok(there) = fs::symlink_metadata(&target) {
            let same = fs::symlink_metadata(&source)
                .is_ok_and(|here| here.dev() == there.dev() && here.ino() == there.ino());
            if !same {
                return Err(format!("{to} already exists"));
            }
        }
        let fail = |why: io::Error| format!("{from} could not be moved to {to}: {}", said(&why));
        fs::create_dir_all(paths::dirname(&target)).map_err(fail)?;
        fs::rename(&source, &target).map_err(fail)?;
        self.forget();
        Ok(Renamed {
            from: self.name(&source),
            to: self.name(&target),
            moved: true,
        })
    }

    /// A file removed, or a directory with what is in it; a link is removed and not followed. The
    /// project, and a directory it is in, are not.
    pub fn remove(&self, path: &str) -> Result<(), String> {
        let absolute = self.absolute(path);
        if self.holds_project(&absolute) {
            return Err("The project itself is not removed".to_owned());
        }
        let fail = |why: io::Error| format!("{path} could not be removed: {}", said(&why));
        let meta = fs::symlink_metadata(&absolute).map_err(fail)?;
        if meta.is_dir() {
            fs::remove_dir_all(&absolute).map_err(fail)?;
        } else {
            fs::remove_file(&absolute).map_err(fail)?;
        }
        self.forget();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_ordered_as_people_read_them() {
        let mut names = vec![
            "file10", "b.txt", "A.txt", "file9", "_x", "file09b", "Zeta", "alpha", "10", "9", "é",
            "a.txt",
        ];
        names.sort_by(|a, b| compare(a, b));
        assert_eq!(
            names,
            [
                "_x", "9", "10", "A.txt", "a.txt", "alpha", "b.txt", "file9", "file09b", "file10",
                "Zeta", "é"
            ]
        );
    }

    #[test]
    fn a_path_travels_relative_to_the_root_inside_it_and_absolute_outside() {
        let tree = Tree::new("/root/project");
        assert_eq!(tree.name("/root/project/src/a.rs"), "src/a.rs");
        assert_eq!(tree.name("/root/project"), "");
        assert_eq!(tree.name("/root/other/a.rs"), "/root/other/a.rs");
        assert_eq!(tree.absolute("src/../b.rs"), "/root/project/b.rs");
    }

    #[test]
    fn the_project_and_what_holds_it_are_not_removed() {
        let tree = Tree::new("/root/project");
        for path in [".", "", "..", "/", "/root"] {
            assert_eq!(
                tree.remove(path),
                Err("The project itself is not removed".to_owned()),
                "{path}"
            );
        }
    }

    /// A tree on `project` inside a directory of a disk, which is the name the disk gives it.
    fn project_in(base: &tempfile::TempDir) -> (std::path::PathBuf, Tree) {
        let base = fs::canonicalize(base.path()).unwrap();
        let project = base.join("up/project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("a.txt"), "a").unwrap();
        let tree = Tree::new(project.to_str().unwrap());
        (base, tree)
    }

    #[test]
    fn a_link_on_the_way_does_not_get_the_project_or_what_holds_it_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (base, tree) = project_in(&dir);
        let project = base.join("up/project");
        // `down` leads to the directory the project is in, `top` to the one that holds that, and
        // `home` to the project itself.
        std::os::unix::fs::symlink(base.join("up"), project.join("down")).unwrap();
        std::os::unix::fs::symlink(&base, project.join("top")).unwrap();
        std::os::unix::fs::symlink(&project, base.join("home")).unwrap();
        let refused = Err("The project itself is not removed".to_owned());
        for path in [
            "down/project",
            "top/up",
            "top/up/project",
            &format!("{}/home/down/project", base.display()),
            &format!("{}/home", base.display()),
        ] {
            let result = tree.remove(path);
            // A link as the last part is only a link: it is removed, and nothing it leads to.
            if path.ends_with("/home") {
                assert_eq!(result, Ok(()), "{path}");
            } else {
                assert_eq!(result, refused, "{path}");
            }
        }
        assert!(project.join("a.txt").exists());
        assert!(!base.join("home").exists());
        // A file in a directory a link leads to is removed as any is.
        std::os::unix::fs::symlink(&project, base.join("again")).unwrap();
        assert_eq!(
            tree.remove(&format!("{}/again/a.txt", base.display())),
            Ok(())
        );
        assert!(!project.join("a.txt").exists());
    }

    #[test]
    fn the_project_is_not_moved_and_a_directory_is_not_moved_into_itself() {
        let dir = tempfile::tempdir().unwrap();
        let (base, tree) = project_in(&dir);
        let project = base.join("up/project");
        std::os::unix::fs::symlink(base.join("up"), project.join("down")).unwrap();
        std::os::unix::fs::symlink(&base, project.join("top")).unwrap();
        for (from, to) in [
            (".", "../moved"),
            ("top/up", "../x"),
            ("down/project", "../x"),
        ] {
            assert_eq!(
                tree.rename(from, to).err(),
                Some("The project itself is not moved".to_owned()),
                "{from}"
            );
        }
        assert!(project.join("a.txt").exists());
        fs::create_dir(project.join("d")).unwrap();
        assert_eq!(
            tree.rename("d", "d/e/f").err(),
            Some("d cannot be moved into itself".to_owned())
        );
        assert!(!project.join("d/e").exists(), "nothing is made on the way");
        assert!(tree.rename("d", "moved").is_ok());
    }

    #[test]
    fn a_name_that_begins_with_two_dots_is_in_the_project() {
        let tree = Tree::new("/root/project");
        assert_eq!(tree.name("/root/project/..hidden"), "..hidden");
        assert_eq!(tree.name("/root/project/..hidden/x"), "..hidden/x");
        assert_eq!(tree.name("/root/other"), "/root/other");
    }
}
