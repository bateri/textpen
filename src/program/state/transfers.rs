//! The files that cross the terminal: a file the person gave the page (dropped, pasted, chosen in
//! the Open panel) is received into the project. The terminal moves the bytes; the program only says
//! where they go, so it never writes a file the person did not ask for, and never one over another
//! unasked.
//!
//! The terminal's side of it is [`Transfer`], which the program's hand on its view is
//! (`pencere::ViewControl`) and which a test puts its own in the place of.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pencere::ViewControl;

use crate::project::paths;
use crate::project::said;
use crate::project::tree::Tree;

/// The kind of thing a folder a file is received into must be.
const NOT_A_FOLDER: &str = "is not a folder of the project";

/// What moves the bytes: the terminal, by the program's hand on its view.
pub trait Transfer: Send + Sync {
    /// Receives the file the person gave by `handle` into `to`, and returns its length. `progress`
    /// is told how many bytes are in and returns `false` to give the receiving up, which deletes
    /// what was received. Where `path` is given (the program reads the same files as the person)
    /// the file is copied from there; else, or where that fails, it crosses the terminal.
    fn receive(
        &self,
        handle: &str,
        path: Option<&str>,
        size: u64,
        to: &Path,
        progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<u64, String>;
}

impl Transfer for ViewControl {
    fn receive(
        &self,
        handle: &str,
        path: Option<&str>,
        size: u64,
        to: &Path,
        progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<u64, String> {
        let received = match path {
            Some(path) => self.receive_from(handle, Path::new(path), size, to, progress),
            None => ViewControl::receive(self, handle, to, progress),
        };
        received.map_err(|why| why.to_string())
    }
}

/// Why a file was not received.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A file is there already, and the page did not say to replace it.
    Exists(String),
    /// Anything else: words for the person.
    Failed(String),
}

impl From<String> for Refusal {
    fn from(words: String) -> Self {
        Self::Failed(words)
    }
}

/// What the page says of a file to receive.
pub struct Incoming<'a> {
    /// The folder, as the project names it.
    pub dir: &'a str,
    /// The name the person's file is to have there (made safe here).
    pub name: &'a str,
    pub handle: &'a str,
    pub size: u64,
    pub path: Option<&'a str>,
    /// An existing file of that name is replaced; else it is left and the file is refused.
    pub replace: bool,
}

/// Where a file goes, and the names that are free.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Place {
    /// The name a file of that name is given (made safe, as the program takes it).
    pub name: String,
    /// Whether the name is taken in the folder (or by one of `also`).
    pub exists: bool,
    /// The name a file is given beside the taken one: `foto 2.png` (`dash`: `image-1.png`).
    pub free: String,
}

pub struct Transfers {
    transfer: Mutex<Arc<dyn Transfer>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The folder `dir` of the project as a path of the disk, if it is a folder and lies in the
/// project, whatever links lead there.
fn folder(project: &Tree, dir: &str) -> Result<PathBuf, String> {
    let absolute = project.absolute(dir);
    let real = |path: &str| {
        fs::canonicalize(path)
            .ok()
            .and_then(|real| real.to_str().map(paths::normalize))
    };
    let (Some(root), Some(inner)) = (real(&project.root), real(&absolute)) else {
        return Err(format!("{dir} {NOT_A_FOLDER}"));
    };
    if !fs::metadata(&absolute).is_ok_and(|meta| meta.is_dir()) || !paths::is_within(&inner, &root)
    {
        return Err(format!("{dir} {NOT_A_FOLDER}"));
    }
    Ok(PathBuf::from(absolute))
}

/// `stem` and `extension` of a name: the last extension, none for a name that is only a dot-name.
fn split(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(at) if at > 0 => name.split_at(at),
        _ => (name, ""),
    }
}

/// The names beside `name` in turn: `foto 2.png`, `foto 3.png`… (`dash`: `foto-1.png`, `foto-2.png`…).
fn variant(name: &str, number: u32, dash: bool) -> String {
    let (stem, extension) = split(name);
    if dash {
        format!("{stem}-{number}{extension}")
    } else {
        format!("{stem} {number}{extension}")
    }
}

impl Transfers {
    pub fn new(transfer: Arc<dyn Transfer>) -> Self {
        Self {
            transfer: Mutex::new(transfer),
        }
    }

    /// Puts another in the place of what moves the files.
    #[cfg(test)]
    pub fn using(&self, transfer: Arc<dyn Transfer>) {
        *lock(&self.transfer) = transfer;
    }

    fn transfer(&self) -> Arc<dyn Transfer> {
        Arc::clone(&lock(&self.transfer))
    }

    /// Whether `name` is taken in `dir` (a link to nothing is taken), and the first free name
    /// beside it. `also` are names a transfer under way is about to take.
    ///
    /// # Errors
    ///
    /// The words for a `dir` that is no folder of the project.
    pub fn place(
        &self,
        project: &Tree,
        dir: &str,
        name: &str,
        also: &[String],
        dash: bool,
    ) -> Result<Place, String> {
        let folder = folder(project, dir)?;
        let name = pencere::safe_name(name);
        let taken = |name: &str| {
            also.iter().any(|each| each == name) || fs::symlink_metadata(folder.join(name)).is_ok()
        };
        let exists = taken(&name);
        let free = if exists {
            (if dash { 1 } else { 2 }..)
                .map(|number| variant(&name, number, dash))
                .find(|candidate| !taken(candidate))
                .unwrap_or_else(|| name.clone())
        } else {
            name.clone()
        };
        Ok(Place { name, exists, free })
    }

    /// Receives the file into the project. The bytes go to a part beside the file, which is given
    /// its name when the last of them is in (`progress` is told how many are, and gives the
    /// receiving up by returning `false`: the part is deleted). A file that is there is replaced
    /// only where `replace` says so; where one comes to be there meanwhile, nothing is replaced.
    ///
    /// Returns the name the file has in the project, and its length.
    ///
    /// # Errors
    ///
    /// [`Refusal::Exists`] for a name that is taken and is not to be replaced, and
    /// [`Refusal::Failed`] with words for the rest.
    pub fn receive(
        &self,
        project: &Tree,
        incoming: &Incoming<'_>,
        progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(String, u64), Refusal> {
        let folder = folder(project, incoming.dir)?;
        let name = pencere::safe_name(incoming.name);
        let target = folder.join(&name);
        let shown = project.name(&target.to_string_lossy());
        match fs::symlink_metadata(&target) {
            Ok(meta) if meta.is_dir() => {
                return Err(Refusal::Failed(format!("{shown} is a folder")));
            }
            Ok(_) if !incoming.replace => return Err(Refusal::Exists(shown)),
            _ => {}
        }
        let transfer = self.transfer();
        let fail = |why: io::Error| Refusal::Failed(format!("{shown}: {}", said(&why)));
        let size = if incoming.replace {
            // Replacing is what was asked: the part takes the name, over the old file.
            transfer.receive(
                incoming.handle,
                incoming.path,
                incoming.size,
                &target,
                progress,
            )?
        } else {
            // A new file is let into its place with a link, which fails where something is there
            // by then: a file that came meanwhile is not written over.
            let staged = folder.join(format!(".{name}.textpen-upload"));
            let size = transfer.receive(
                incoming.handle,
                incoming.path,
                incoming.size,
                &staged,
                progress,
            )?;
            let placed = fs::hard_link(&staged, &target).or_else(|why| {
                if why.kind() == io::ErrorKind::AlreadyExists {
                    Err(why)
                } else if fs::symlink_metadata(&target).is_ok() {
                    Err(io::Error::from(io::ErrorKind::AlreadyExists))
                } else {
                    // A disk that makes no links: the file is renamed, in the moment between.
                    fs::rename(&staged, &target)
                }
            });
            let _ = fs::remove_file(&staged);
            match placed {
                Ok(()) => size,
                Err(why) if why.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(Refusal::Exists(shown));
                }
                Err(why) => return Err(fail(why)),
            }
        };
        project.forget();
        Ok((shown, size))
    }
}

/// What moves the files, as a test sees it: files by handle held in memory, copied where a path
/// is given, in pieces the test sets, and held (`hold`) at a byte until the test lets them on.
#[cfg(test)]
pub mod double {
    use std::collections::HashMap;
    use std::sync::Condvar;
    use std::time::Duration;

    use super::{Mutex, Path, Transfer, fs, lock};

    #[derive(Default)]
    struct Gate {
        /// The bytes a receiving may go to; none: as far as the file does.
        until: Option<u64>,
        /// The receivings that are under way.
        going: u32,
    }

    #[derive(Default)]
    pub struct Double {
        /// The files the person gave, by handle.
        pub given: Mutex<HashMap<String, Vec<u8>>>,
        /// How many bytes make a piece: a progress report is told after each.
        pub piece: Mutex<usize>,
        gate: Mutex<Gate>,
        moved: Condvar,
        /// The paths asked for, in order (`None`: through the terminal).
        pub routes: Mutex<Vec<Option<String>>>,
    }

    impl Double {
        pub fn give(&self, handle: &str, bytes: &[u8]) {
            lock(&self.given).insert(handle.to_owned(), bytes.to_vec());
        }

        /// A receiving waits where it has `bytes` in, until [`Double::go_on`].
        pub fn hold_at(&self, bytes: u64) {
            lock(&self.gate).until = Some(bytes);
        }

        /// Lets the receivings on, to their ends.
        pub fn go_on(&self) {
            lock(&self.gate).until = None;
            self.moved.notify_all();
        }

        /// How many receivings are under way.
        pub fn going(&self) -> u32 {
            lock(&self.gate).going
        }

        pub fn routes(&self) -> Vec<Option<String>> {
            lock(&self.routes).clone()
        }
    }

    impl Transfer for Double {
        fn receive(
            &self,
            handle: &str,
            path: Option<&str>,
            _size: u64,
            to: &Path,
            progress: &mut dyn FnMut(u64) -> bool,
        ) -> Result<u64, String> {
            lock(&self.routes).push(path.map(str::to_owned));
            let bytes = match path.and_then(|path| fs::read(path).ok()) {
                Some(bytes) => bytes,
                None => lock(&self.given)
                    .get(handle)
                    .cloned()
                    .ok_or_else(|| "no such handle".to_owned())?,
            };
            let part = to.with_file_name(format!(
                ".{}.pencere-part",
                to.file_name().unwrap_or_default().to_string_lossy()
            ));
            fs::write(&part, b"").map_err(|why| why.to_string())?;
            lock(&self.gate).going += 1;
            let piece = match *lock(&self.piece) {
                0 => usize::MAX,
                piece => piece,
            };
            let outcome = (|| {
                let mut done = 0_usize;
                loop {
                    // Held where the test says: the terminal has not answered yet, so the program
                    // hears nothing (and sees no cancel) until the test lets the bytes on.
                    let limit = {
                        let mut gate = lock(&self.gate);
                        loop {
                            match gate.until {
                                Some(until) if done as u64 >= until && done < bytes.len() => {
                                    gate = self
                                        .moved
                                        .wait_timeout(gate, Duration::from_millis(20))
                                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                                        .0;
                                }
                                Some(until) => break usize::try_from(until).unwrap_or(usize::MAX),
                                None => break usize::MAX,
                            }
                        }
                    };
                    let upto = done.saturating_add(piece).min(limit).min(bytes.len());
                    if let Some(slice) = bytes.get(done..upto) {
                        use std::io::Write as _;
                        fs::OpenOptions::new()
                            .append(true)
                            .open(&part)
                            .and_then(|mut file| file.write_all(slice))
                            .map_err(|why| why.to_string())?;
                    }
                    done = upto;
                    if !progress(done as u64) {
                        return Err("given up".to_owned());
                    }
                    if done >= bytes.len() {
                        return fs::rename(&part, to)
                            .map(|()| done as u64)
                            .map_err(|why| why.to_string());
                    }
                }
            })();
            lock(&self.gate).going -= 1;
            if outcome.is_err() {
                let _ = fs::remove_file(&part);
            }
            outcome
        }
    }
}

#[cfg(test)]
mod tests {
    use super::double::Double;
    use super::*;

    fn project() -> (tempfile::TempDir, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lantern");
        fs::create_dir_all(root.join("docs")).unwrap();
        let tree = Tree::new(&root.to_string_lossy());
        (dir, tree)
    }

    fn transfers() -> (Transfers, Arc<Double>) {
        let double = Arc::new(Double::default());
        (
            Transfers::new(Arc::clone(&double) as Arc<dyn Transfer>),
            double,
        )
    }

    fn incoming<'a>(dir: &'a str, name: &'a str, handle: &'a str, replace: bool) -> Incoming<'a> {
        Incoming {
            dir,
            name,
            handle,
            size: 0,
            path: None,
            replace,
        }
    }

    #[test]
    fn the_names_beside_a_taken_one_are_counted_from_two_with_a_space_or_from_one_with_a_dash() {
        assert_eq!(variant("foto.png", 2, false), "foto 2.png");
        assert_eq!(variant("image.png", 1, true), "image-1.png");
        assert_eq!(variant("notes", 2, false), "notes 2");
        assert_eq!(variant("a.tar.gz", 2, false), "a.tar 2.gz");
        assert_eq!(variant(".env", 2, false), ".env 2");
    }

    #[test]
    fn a_name_is_taken_by_a_file_a_link_or_a_transfer_under_way_and_the_next_free_one_is_told() {
        let (dir, tree) = project();
        let root = dir.path().join("lantern");
        fs::write(root.join("docs/foto.png"), b"x").unwrap();
        fs::write(root.join("docs/foto 2.png"), b"x").unwrap();
        std::os::unix::fs::symlink("nowhere", root.join("docs/gone.png")).unwrap();
        let (transfers, _) = transfers();
        let place = |name: &str, also: &[&str], dash: bool| {
            let also: Vec<String> = also.iter().map(|name| (*name).to_owned()).collect();
            transfers.place(&tree, "docs", name, &also, dash).unwrap()
        };
        assert_eq!(
            place("foto.png", &[], false),
            Place {
                name: "foto.png".to_owned(),
                exists: true,
                free: "foto 3.png".to_owned()
            }
        );
        assert_eq!(
            place("new.png", &[], false),
            Place {
                name: "new.png".to_owned(),
                exists: false,
                free: "new.png".to_owned()
            }
        );
        assert!(place("gone.png", &[], false).exists, "a link to nothing");
        let busy = place("new.png", &["new.png", "new 2.png"], false);
        assert_eq!((busy.exists, busy.free.as_str()), (true, "new 3.png"));
        assert_eq!(place("foto.png", &[], true).free, "foto-1.png");
        assert!(transfers.place(&tree, "nothing", "a", &[], false).is_err());
        assert!(transfers.place(&tree, "/etc", "a", &[], false).is_err());
    }

    #[test]
    fn a_file_is_received_beside_its_place_and_named_when_it_is_whole() {
        let (dir, tree) = project();
        let (transfers, double) = transfers();
        double.give("h1", b"hello");
        let got = transfers
            .receive(&tree, &incoming("docs", "a.txt", "h1", false), &mut |_| {
                true
            })
            .unwrap();
        assert_eq!(got, ("docs/a.txt".to_owned(), 5));
        let root = dir.path().join("lantern/docs");
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"hello");
        let left: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["a.txt"], "no part and no staging left");
    }

    #[test]
    fn a_file_that_is_there_is_never_written_over_unless_it_is_to_be_replaced() {
        let (dir, tree) = project();
        let (transfers, double) = transfers();
        let root = dir.path().join("lantern/docs");
        fs::write(root.join("a.txt"), b"mine").unwrap();
        double.give("h", b"theirs");
        let refused =
            transfers.receive(&tree, &incoming("docs", "a.txt", "h", false), &mut |_| true);
        assert_eq!(refused, Err(Refusal::Exists("docs/a.txt".to_owned())));
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"mine");
        assert!(double.routes().is_empty(), "no byte was asked for");
        // A name that is only a safe form of what came: nothing climbs out.
        let climbing = transfers
            .receive(
                &tree,
                &incoming("docs", "../../outside.txt", "h", false),
                &mut |_| true,
            )
            .unwrap();
        assert_eq!(climbing.0, "docs/outside.txt");
        assert!(!dir.path().join("outside.txt").exists());
        // Replacing is said.
        transfers
            .receive(&tree, &incoming("docs", "a.txt", "h", true), &mut |_| true)
            .unwrap();
        assert_eq!(fs::read(root.join("a.txt")).unwrap(), b"theirs");
        // A folder is not replaced by a file.
        fs::create_dir(root.join("sub")).unwrap();
        assert!(matches!(
            transfers.receive(&tree, &incoming("docs", "sub", "h", true), &mut |_| true),
            Err(Refusal::Failed(_))
        ));
    }

    #[test]
    fn a_file_that_comes_to_be_there_while_another_is_received_is_not_written_over() {
        let (dir, tree) = project();
        let (transfers, double) = transfers();
        let root = dir.path().join("lantern/docs");
        double.give("h", b"theirs");
        let target = root.join("race.txt");
        let refused = transfers.receive(
            &tree,
            &incoming("docs", "race.txt", "h", false),
            &mut |_| {
                // Somebody makes the file while the bytes are on their way.
                let _ = fs::write(&target, b"first");
                true
            },
        );
        assert_eq!(refused, Err(Refusal::Exists("docs/race.txt".to_owned())));
        assert_eq!(fs::read(&target).unwrap(), b"first");
        let names: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["race.txt"], "the staged copy is deleted");
    }

    #[test]
    fn a_receiving_given_up_leaves_no_part() {
        let (dir, tree) = project();
        let (transfers, double) = transfers();
        *double.piece.lock().unwrap() = 2;
        double.give("h", b"abcdefgh");
        let mut calls = 0;
        let got = transfers.receive(&tree, &incoming("docs", "a.txt", "h", false), &mut |_| {
            calls += 1;
            calls < 2
        });
        assert!(matches!(got, Err(Refusal::Failed(_))));
        let left = fs::read_dir(dir.path().join("lantern/docs"))
            .unwrap()
            .count();
        assert_eq!(left, 0);
    }

    #[test]
    fn a_folder_outside_the_project_or_a_name_that_is_not_one_is_refused() {
        let (dir, tree) = project();
        let (transfers, double) = transfers();
        double.give("h", b"x");
        std::os::unix::fs::symlink(dir.path(), dir.path().join("lantern/out")).unwrap();
        for bad in ["/etc", "../", "out", "docs/none"] {
            let got = transfers.receive(&tree, &incoming(bad, "a.txt", "h", false), &mut |_| true);
            assert!(matches!(got, Err(Refusal::Failed(_))), "{bad}");
        }
        assert!(!dir.path().join("a.txt").exists());
    }
}
