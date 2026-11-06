//! The pictures the page draws: the program reads a picture file, tells the page how large it is,
//! and gives the file to the terminal to hold (`attach`), which the page then draws by the name it
//! is given (`sha256:` and the file's digest). Each picture is held for an owner, a tab of the page
//! (a picture tab, or a Markdown file shown with its preview), and let go (`detach`) when the last
//! owner that held it lets go of it, which is when its tab closes.
//!
//! A PDF is a picture too: the terminal draws its pages, and the program keeps it read, for the
//! owner that shows it, to say where its words are (`pdf-text`) and to search it (`pdf-find`). A
//! PDF is a picture of a tab and not of a Markdown file: what a Markdown file shows is an image.
//!
//! The terminal's side of it is [`Attach`], which the program's hand on its view is
//! (`pencere::ViewControl`) and which a test puts its own in the place of.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read as _;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pencere::ViewControl;
use serde::Serialize;

use crate::project::image::{self, BYTES_LIMIT, SIDE_LIMIT, Size};
use crate::project::paths;
use crate::project::pdf::{self, Document, Unopened};
use crate::project::tree::Tree;

/// What holds the files the page draws: the terminal, by the program's hand on its view.
pub trait Attach: Send + Sync {
    /// Gives `bytes` to the terminal and returns the name the page draws them by.
    fn attach(&self, bytes: Vec<u8>) -> Result<String, String>;

    /// Lets the file of `name` go.
    fn detach(&self, name: &str);
}

impl Attach for ViewControl {
    fn attach(&self, bytes: Vec<u8>) -> Result<String, String> {
        ViewControl::attach(self, bytes).map_err(|why| why.to_string())
    }

    fn detach(&self, name: &str) {
        ViewControl::detach(self, name);
    }
}

/// A picture the page can draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Picture {
    /// What the page draws it by.
    pub name: String,
    /// The file's bytes.
    pub size: u64,
    /// In pixels; of a PDF, the first page's, in points.
    pub width: u32,
    pub height: u32,
    /// What a PDF has besides: its pages and whether it has words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<Pages>,
}

/// The pages of a PDF, for the page to lay out before it asks for any of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pages {
    /// How many pages there are.
    pub pages: usize,
    /// Their sizes in points, as runs of pages of one size: `[how many, width, height]`.
    pub sizes: Vec<(usize, f64, f64)>,
    /// Whether any page has text to select and to search.
    pub text: bool,
}

/// Why a file is not shown as a picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// It is a picture, and larger than the terminal draws: more bytes or more pixels.
    TooLarge { size: u64 },
    /// It is none: not there, not a file, not of a name or a format of a picture, not readable, or
    /// (for a Markdown file's picture) not in the project.
    NotAPicture,
}

pub struct Pictures {
    attach: Mutex<Arc<dyn Attach>>,
    /// The names each owner holds.
    held: Mutex<BTreeMap<String, BTreeSet<String>>>,
    /// The PDF each owner shows, read.
    documents: Mutex<BTreeMap<String, Arc<Document>>>,
}

impl Pictures {
    pub fn new(attach: Arc<dyn Attach>) -> Self {
        Self {
            attach: Mutex::new(attach),
            held: Mutex::new(BTreeMap::new()),
            documents: Mutex::new(BTreeMap::new()),
        }
    }

    /// Puts another in the place of what holds the files.
    #[cfg(test)]
    pub fn attaching(&self, attach: Arc<dyn Attach>) {
        *lock(&self.attach) = attach;
    }

    /// The picture at `path`, held for `owner`. `path` is the project's, or absolute; where `from`
    /// is given it is a Markdown file's and `path` is as that file wrote it: relative to the file's
    /// folder, in the project and nowhere else, and not an address.
    ///
    /// # Errors
    ///
    /// The words for a terminal that would not hold the file.
    pub fn show(
        &self,
        project: &Tree,
        path: &str,
        from: Option<&str>,
        owner: &str,
    ) -> Result<Result<Picture, Refusal>, String> {
        let Some(absolute) = (match from {
            Some(from) => written(project, from, path),
            None => Some(project.absolute(path)),
        }) else {
            return Ok(Err(Refusal::NotAPicture));
        };
        // A Markdown file's picture is read from the project and from nowhere else, with every link
        // on the way followed; nothing is read of a file outside it.
        if from.is_some() && !inside(project, &absolute) {
            return Ok(Err(Refusal::NotAPicture));
        }
        let read = match read(&absolute) {
            Ok(read) => read,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let (bytes, size, content) = read;
        let (width, height, document) = match content {
            Content::Image(Size { width, height }) => {
                if width > SIDE_LIMIT || height > SIDE_LIMIT {
                    return Ok(Err(Refusal::TooLarge { size }));
                }
                (width, height, None)
            }
            Content::Pdf(_) if from.is_some() => return Ok(Err(Refusal::NotAPicture)),
            Content::Pdf(document) => {
                let (width, height) = document.sizes().first().copied().unwrap_or((1.0, 1.0));
                (whole(width), whole(height), Some(document))
            }
        };
        let name = lock(&self.attach).attach(bytes)?;
        lock(&self.held)
            .entry(owner.to_owned())
            .or_default()
            .insert(name.clone());
        let pdf = document.map(|document| {
            let pages = Pages {
                pages: document.sizes().len(),
                sizes: document.runs(),
                text: document.has_text(),
            };
            lock(&self.documents).insert(owner.to_owned(), Arc::new(document));
            pages
        });
        if pdf.is_none() {
            // What the owner showed before is not what it shows now.
            lock(&self.documents).remove(owner);
        }
        Ok(Ok(Picture {
            name,
            size,
            width,
            height,
            pdf,
        }))
    }

    /// The PDF `owner` shows, if it shows one.
    pub fn document(&self, owner: &str) -> Option<Arc<Document>> {
        lock(&self.documents).get(owner).cloned()
    }

    /// `owner` holds no picture any more: the files nobody else holds are let go.
    pub fn release(&self, owner: &str) {
        lock(&self.documents).remove(owner);
        let mut held = lock(&self.held);
        let Some(names) = held.remove(owner) else {
            return;
        };
        let gone: Vec<String> = names
            .into_iter()
            .filter(|name| !held.values().any(|others| others.contains(name)))
            .collect();
        drop(held);
        let attach = Arc::clone(&lock(&self.attach));
        for name in gone {
            attach.detach(&name);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Where the picture `src`, as a Markdown file `from` wrote it, is: the folder of the file, then
/// the path. None for what is not a path to a file of the project's disk: an address (`https:`,
/// `data:`), an absolute path, a part of the page (`#top`), or nothing.
fn written(project: &Tree, from: &str, src: &str) -> Option<String> {
    let src = src.split(['#', '?']).next().unwrap_or_default();
    let src = decoded(src)?;
    let scheme = src.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "+-.".contains(ch))
    });
    if src.is_empty() || src.starts_with('/') || src.starts_with('\\') || scheme {
        return None;
    }
    let folder = paths::dirname(&project.absolute(from));
    Some(paths::resolve(&folder, &src))
}

/// Text with its `%20`s read; none for one that is not UTF-8 after.
fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let digits = text.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(digits, 16).ok()?);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Whether `absolute`, with every link on the way followed, is in the project's folder.
fn inside(project: &Tree, absolute: &str) -> bool {
    let (Ok(file), Ok(root)) = (fs::canonicalize(absolute), fs::canonicalize(&project.root)) else {
        return false;
    };
    file.starts_with(root)
}

/// What a picture file is: an image of a size in pixels, or a PDF, read.
enum Content {
    Image(Size),
    Pdf(Document),
}

/// A page's size in points as the whole number the picture says it is: never below one.
fn whole(points: f64) -> u32 {
    if points.is_finite() && points >= 1.0 {
        points.ceil() as u32
    } else {
        1
    }
}

/// A picture file's bytes, how many bytes it is, and what it is.
fn read(absolute: &str) -> Result<(Vec<u8>, u64, Content), Refusal> {
    if !image::is_named_so(absolute) {
        return Err(Refusal::NotAPicture);
    }
    let meta = fs::metadata(absolute).map_err(|_| Refusal::NotAPicture)?;
    if !meta.is_file() {
        return Err(Refusal::NotAPicture);
    }
    if meta.len() > BYTES_LIMIT {
        return Err(Refusal::TooLarge { size: meta.len() });
    }
    // No more is read than the limit allows (and one byte more, to know a file grew past it since
    // its size was asked).
    let mut bytes = Vec::new();
    fs::File::open(absolute)
        .and_then(|file| file.take(BYTES_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|_| Refusal::NotAPicture)?;
    let size = bytes.len() as u64;
    if size > BYTES_LIMIT {
        return Err(Refusal::TooLarge { size });
    }
    // The terminal tells a PDF by the bytes and not by the name, and so does this.
    if pdf::is_pdf(&bytes) {
        return match Document::open(Arc::from(bytes.as_slice())) {
            Ok(document) => Ok((bytes, size, Content::Pdf(document))),
            // Too long to lay out in one message: as large as a file that is too large to read.
            Err(Unopened::TooManyPages) => Err(Refusal::TooLarge { size }),
            Err(Unopened::Unreadable) => Err(Refusal::NotAPicture),
        };
    }
    let dimensions = image::size_of(&bytes).ok_or(Refusal::NotAPicture)?;
    Ok((bytes, size, Content::Image(dimensions)))
}

/// What holds the files, as a test sees it: it makes the name the terminal would and keeps the
/// names it was given and the ones it let go.
#[cfg(test)]
pub mod recorder {
    use super::{Attach, Mutex, lock};
    use sha2::{Digest as _, Sha256};

    #[derive(Default)]
    pub struct Recorder {
        pub attached: Mutex<Vec<String>>,
        pub detached: Mutex<Vec<String>>,
    }

    /// The name the terminal gives a file: `sha256:` and the digest of its bytes.
    pub fn digest_name(bytes: &[u8]) -> String {
        format!("sha256:{:x}", Sha256::digest(bytes))
    }

    impl Recorder {
        pub fn attached(&self) -> Vec<String> {
            lock(&self.attached).clone()
        }

        pub fn detached(&self) -> Vec<String> {
            lock(&self.detached).clone()
        }
    }

    impl Attach for Recorder {
        fn attach(&self, bytes: Vec<u8>) -> Result<String, String> {
            let name = digest_name(&bytes);
            lock(&self.attached).push(name.clone());
            Ok(name)
        }

        fn detach(&self, name: &str) {
            lock(&self.detached).push(name.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recorder::{Recorder, digest_name};
    use super::*;

    /// Three pages, 612 by 792, 595 by 842 and 300 by 200 points; the first two have words.
    const PAPER: &[u8] = include_bytes!("../../../tests/fixtures/pages.pdf");

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    fn project(files: &[(&str, Vec<u8>)]) -> (tempfile::TempDir, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lantern");
        for (path, bytes) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        let tree = Tree::new(&root.to_string_lossy());
        (dir, tree)
    }

    fn pictures() -> (Pictures, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        (
            Pictures::new(Arc::clone(&recorder) as Arc<dyn Attach>),
            recorder,
        )
    }

    #[test]
    fn a_picture_is_shown_by_the_name_the_terminal_holds_it_under() {
        let bytes = png_of(640, 480);
        let (_dir, tree) = project(&[("logo.png", bytes.clone())]);
        let (pictures, recorder) = pictures();
        let shown = pictures.show(&tree, "logo.png", None, "logo.png").unwrap();
        assert_eq!(
            shown,
            Ok(Picture {
                name: digest_name(&bytes),
                size: bytes.len() as u64,
                width: 640,
                height: 480,
                pdf: None,
            })
        );
        assert_eq!(recorder.attached(), [digest_name(&bytes)]);
    }

    #[test]
    fn a_file_that_is_no_picture_or_too_large_a_one_is_refused_before_the_terminal_is_given_it() {
        let (_dir, tree) = project(&[
            ("wide.png", png_of(4097, 10)),
            ("tall.png", png_of(10, 4097)),
            ("limit.png", png_of(4096, 4096)),
            ("text.png", b"not a picture".to_vec()),
            ("notes.txt", png_of(5, 5)),
            ("big.png", {
                let mut bytes = png_of(5, 5);
                bytes.resize(8 * 1024 * 1024 + 1, 0);
                bytes
            }),
        ]);
        let (pictures, recorder) = pictures();
        let show = |path: &str| pictures.show(&tree, path, None, "owner").unwrap();
        assert!(matches!(show("wide.png"), Err(Refusal::TooLarge { .. })));
        assert!(matches!(show("tall.png"), Err(Refusal::TooLarge { .. })));
        assert_eq!(
            show("big.png"),
            Err(Refusal::TooLarge {
                size: 8 * 1024 * 1024 + 1
            })
        );
        assert_eq!(show("text.png"), Err(Refusal::NotAPicture));
        assert_eq!(show("notes.txt"), Err(Refusal::NotAPicture));
        assert_eq!(show("gone.png"), Err(Refusal::NotAPicture));
        assert_eq!(show(""), Err(Refusal::NotAPicture));
        // The edge is a picture: 4096 either way.
        assert!(show("limit.png").is_ok());
        assert_eq!(recorder.attached().len(), 1);
    }

    #[test]
    fn a_pictures_file_is_let_go_when_the_last_who_held_it_lets_go() {
        let (_dir, tree) = project(&[("a.png", png_of(3, 3)), ("b.png", png_of(4, 4))]);
        let (pictures, recorder) = pictures();
        let a = pictures
            .show(&tree, "a.png", None, "one")
            .unwrap()
            .unwrap()
            .name;
        pictures.show(&tree, "a.png", None, "two").unwrap().unwrap();
        let b = pictures
            .show(&tree, "b.png", None, "one")
            .unwrap()
            .unwrap()
            .name;
        pictures.release("one");
        // `a` is still held by `two`.
        assert_eq!(recorder.detached(), [b]);
        pictures.release("two");
        assert_eq!(recorder.detached().last(), Some(&a));
        // Nothing is held by an owner that holds none, twice or not at all.
        pictures.release("two");
        pictures.release("nobody");
        assert_eq!(recorder.detached().len(), 2);
    }

    #[test]
    fn a_markdown_files_picture_is_the_one_beside_it_in_the_project_and_nowhere_else() {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.png"), png_of(2, 2)).unwrap();
        let (dir, tree) = project(&[
            ("docs/x.png", png_of(7, 5)),
            ("docs/a b.png", png_of(8, 5)),
            ("top.png", png_of(9, 5)),
            ("docs/sub/note.md", b"".to_vec()),
        ]);
        std::os::unix::fs::symlink(
            outside.path().join("secret.png"),
            dir.path().join("lantern/docs/link.png"),
        )
        .unwrap();
        let (pictures, _recorder) = pictures();
        let show = |from: &str, src: &str| {
            pictures
                .show(&tree, src, Some(from), "docs/sub/note.md")
                .unwrap()
                .map(|picture| (picture.width, picture.height))
        };
        // Relative to the file's folder, and up from it.
        assert_eq!(show("docs/sub/note.md", "../x.png"), Ok((7, 5)));
        assert_eq!(show("docs/sub/note.md", "../../top.png"), Ok((9, 5)));
        assert_eq!(show("docs/sub/note.md", "../a%20b.png"), Ok((8, 5)));
        assert_eq!(show("docs/sub/note.md", "./../x.png?raw=1#top"), Ok((7, 5)));
        // The same from the project's top, where the file is.
        assert_eq!(show("top.md", "docs/x.png"), Ok((7, 5)));
        // Out of the project: by dots, by a path from the top of the disk, by a link.
        let climbing = "../../../secret.png";
        assert_eq!(
            show("docs/sub/note.md", climbing),
            Err(Refusal::NotAPicture)
        );
        let escaped = outside.path().join("secret.png");
        assert_eq!(
            show("docs/sub/note.md", &escaped.to_string_lossy()),
            Err(Refusal::NotAPicture)
        );
        assert_eq!(show("docs/note.md", "link.png"), Err(Refusal::NotAPicture));
        // Addresses and the like are no files.
        for src in [
            "https://example.com/x.png",
            "data:image/png;base64,AAAA",
            "file:///etc/x.png",
            "#top",
            "",
            "%zz.png",
        ] {
            assert_eq!(
                show("docs/note.md", src),
                Err(Refusal::NotAPicture),
                "{src}"
            );
        }
    }

    #[test]
    fn a_pdf_is_held_read_for_the_owner_that_shows_it_and_let_go_with_it() {
        let (_dir, tree) = project(&[
            ("paper.pdf", PAPER.to_vec()),
            ("logo.png", png_of(3, 3)),
            ("docs/guide.md", b"".to_vec()),
            ("docs/paper.pdf", PAPER.to_vec()),
            ("words.pdf", b"%PDF-1.4 and then words".to_vec()),
        ]);
        let (pictures, recorder) = pictures();
        let shown = pictures
            .show(&tree, "paper.pdf", None, "tab")
            .unwrap()
            .unwrap();
        assert_eq!(shown.name, digest_name(PAPER));
        assert_eq!(shown.size, PAPER.len() as u64);
        // The first page's size, then the pages by runs of one size.
        assert_eq!((shown.width, shown.height), (612, 792));
        assert_eq!(
            shown.pdf,
            Some(Pages {
                pages: 3,
                sizes: vec![(1, 612.0, 792.0), (1, 595.0, 842.0), (1, 300.0, 200.0)],
                text: true,
            })
        );
        assert!(pictures.document("tab").is_some());
        assert!(pictures.document("other").is_none());
        // Shown again it is the same file under the same name and read again; let go, it is gone
        // from the terminal and from the program.
        pictures
            .show(&tree, "paper.pdf", None, "tab")
            .unwrap()
            .unwrap();
        pictures.release("tab");
        assert!(pictures.document("tab").is_none());
        assert_eq!(recorder.detached(), [digest_name(PAPER)]);
        // What a tab shows in place of its PDF is not a PDF.
        pictures
            .show(&tree, "paper.pdf", None, "tab")
            .unwrap()
            .unwrap();
        let logo = pictures
            .show(&tree, "logo.png", None, "tab")
            .unwrap()
            .unwrap();
        assert_eq!(logo.pdf, None);
        assert!(pictures.document("tab").is_none());
        // A Markdown file shows images, and a PDF is none; a PDF that cannot be read is none.
        let attached = recorder.attached().len();
        let from = |src: &str| {
            pictures
                .show(&tree, src, Some("docs/guide.md"), "docs/guide.md")
                .unwrap()
        };
        assert_eq!(from("paper.pdf"), Err(Refusal::NotAPicture));
        assert_eq!(
            pictures.show(&tree, "words.pdf", None, "words").unwrap(),
            Err(Refusal::NotAPicture)
        );
        assert_eq!(recorder.attached().len(), attached);
        assert!(pictures.document("docs/guide.md").is_none());
    }
}
