//! The page (src/view/) and the program's own handlers in one process, as a terminal runs them: the
//! kit's interpreter and editors under pencere's host with no screen (`pencere-drive`), a clock the
//! test moves, the calls of `api/` served by `pencere::Conversation` as `serve` serves them — the
//! handlers `main` gives a terminal (`options`), on a project of the test's own. What the page shows
//! is read as words; what the person does is done to it as a terminal does it. A call of the page is
//! held to the kit's 64 MiB, and to its 250 ms a call when asked (`make check-timing`; `limits`).
//!
//! Each opens the interpreter (compiled once a run): `make test-page` runs them, in `make check`.

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere_drive::headless::{Error, Focus, Headless, InProcess};
use pencere_view::{Id, MenuEntry, Paint, Point, Shortcut};
use serde_json::Value;

use crate::options;
use crate::state::defaults::{Place, UiState};
use crate::state::pictures::recorder::Recorder;
use crate::state::session::Session;
use crate::state::store::Store;

/// A page as large as the docs' pictures, and an 80×24 pane.
pub(crate) const WIDE: (f32, f32) = (1100.0, 720.0);
pub(crate) const PANE: (f32, f32) = (600.0, 330.0);

/// How long a test waits for what the program says on its own (the disk looked at, a search).
const WITHIN: Duration = Duration::from_secs(20);

pub(crate) type Page = Headless<InProcess>;

/// A project of the test's own, with its settings and drafts beside it.
pub(crate) struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("a directory");
        let project = Self { dir };
        std::fs::create_dir_all(project.root()).expect("the project");
        for (path, text) in files {
            project.write(path, text);
        }
        project
    }

    pub(crate) fn root(&self) -> PathBuf {
        self.dir.path().join("lantern")
    }

    pub(crate) fn write(&self, path: &str, text: &str) {
        let file = self.root().join(path);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).expect("its folder");
        }
        std::fs::write(file, text).expect("the file");
    }

    pub(crate) fn write_bytes(&self, path: &str, bytes: &[u8]) {
        let file = self.root().join(path);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).expect("its folder");
        }
        std::fs::write(file, bytes).expect("the file");
    }

    pub(crate) fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path)).unwrap_or_default()
    }

    pub(crate) fn exists(&self, path: &str) -> bool {
        self.root().join(path).exists()
    }

    /// The program's state of this project, as a run of textpen in it makes it; the page opens on
    /// `tabs`, the first in front.
    pub(crate) fn session(&self, tabs: &[&str]) -> Arc<Session> {
        let path = |name: &str| self.dir.path().join(name).to_string_lossy().into_owned();
        let store = Store::at(path("config"), path("state"));
        let session = Arc::new(Session::new(
            &self.root().to_string_lossy(),
            &[],
            None,
            store,
        ));
        if !tabs.is_empty() {
            session.keep(UiState {
                tabs: tabs.iter().map(|&tab| tab.to_owned()).collect(),
                active: tabs.first().map(|&tab| tab.to_owned()),
                recent: tabs.iter().map(|&tab| tab.to_owned()).collect(),
                ..UiState::default()
            });
        }
        session
    }

    /// The page opened on the program's handlers for `session`, at `size`.
    pub(crate) fn open(&self, session: &Arc<Session>, size: (f32, f32)) -> Result<Page, Error> {
        self.open_later(session, size, 0.0)
    }

    /// As [`Project::open`], the machine's time of day `later` seconds past the real one.
    /// As [`Project::open`], on a terminal that holds `files` already (pictures attached).
    pub(crate) fn open_holding(
        &self,
        session: &Arc<Session>,
        size: (f32, f32),
        files: &[&[u8]],
    ) -> Result<Page, Error> {
        let program = InProcess::new(options(session, &[], &Arc::new(Mutex::new(None))));
        open_holding(program, size, None, 0.0, files)
    }

    pub(crate) fn open_later(
        &self,
        session: &Arc<Session>,
        size: (f32, f32),
        later: f64,
    ) -> Result<Page, Error> {
        let program = InProcess::new(options(session, &[], &Arc::new(Mutex::new(None))));
        open_after(program, size, None, later)
    }
}

/// What the page's module is held to. The tests say what the page shows and does, not how fast a
/// machine is: a runner under load takes ten times as long as one at rest, so the kit's 250 ms a
/// call and 2 s to start, which are the wall clock's, are far wider here by default
/// (`GENEROUS_CALL`). With `TEXTPEN_CALL_MS` set (`make check-timing`) a call is held to that
/// many milliseconds, the rest to the kit's own: the page in the time a terminal gives it, or in
/// half of it (`TEXTPEN_CALL_MS=125`), to see how near it comes. Meant for a machine at rest.
pub(crate) fn limits(call_ms: Option<u64>) -> pencere_sandbox::ModuleLimits {
    let kit = pencere_sandbox::ModuleLimits::floor();
    match call_ms {
        Some(ms) => pencere_sandbox::ModuleLimits {
            call: Duration::from_millis(ms),
            ..kit
        },
        None => pencere_sandbox::ModuleLimits {
            call: GENEROUS_CALL,
            start: GENEROUS_CALL * 6,
            ..kit
        },
    }
}

/// A call of the page, in a test that is not about time: forty times the kit's.
const GENEROUS_CALL: Duration = Duration::from_secs(10);

/// The page of this checkout's src/view/ (and its pictures, src/assets/), on `program`.
pub(crate) fn open_with(
    program: InProcess,
    size: (f32, f32),
    fonts: Option<&pencere_drive::shot::Fonts>,
) -> Result<Page, Error> {
    open_after(program, size, fonts, 0.0)
}

/// As [`open_with`], on a machine whose time of day is `later` seconds past the real one (the page
/// tells "4 minutes ago" from the host's time of day, `pencere.time.wall()`).
pub(crate) fn open_after(
    program: InProcess,
    size: (f32, f32),
    fonts: Option<&pencere_drive::shot::Fonts>,
    later: f64,
) -> Result<Page, Error> {
    open_holding(program, size, fonts, later, &[])
}

/// As [`open_after`], on a terminal that holds `files` already: the pictures the program attached
/// before the page came, which a page that starts on a picture draws from its first frame.
pub(crate) fn open_holding(
    program: InProcess,
    size: (f32, f32),
    fonts: Option<&pencere_drive::shot::Fonts>,
    later: f64,
    files: &[&[u8]],
) -> Result<Page, Error> {
    open_patiently(program, size, fonts, later, files, None)
}

/// As [`open_holding`], the host giving a program that owes the page an answer `patience` to give it
/// (the default is a minute): a test that holds a call half way (a file on its way) is not made to
/// wait for it.
pub(crate) fn open_patiently(
    program: InProcess,
    size: (f32, f32),
    fonts: Option<&pencere_drive::shot::Fonts>,
    later: f64,
    files: &[&[u8]],
    patience: Option<Duration>,
) -> Result<Page, Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64());
    let mut opening = Headless::app(env!("CARGO_MANIFEST_DIR"))
        .size(size.0, size.1)
        .wall_clock(now + later, 0);
    opening = opening.limits(limits(
        std::env::var("TEXTPEN_CALL_MS")
            .ok()
            .and_then(|ms| ms.parse().ok()),
    ));
    if let Some(fonts) = fonts {
        opening = opening.measure(fonts.clone()).scale(2.0);
    }
    for bytes in files {
        opening = opening.attached(bytes);
    }
    if let Some(patience) = patience {
        opening = opening.patience(patience);
    }
    let mut page = opening.peer(program).open()?;
    // What the page reads first comes from the program on its own threads: the file in front, or
    // with none the start page, or a picture.
    page.wait_for(WITHIN, |page| {
        shown(page).is_some()
            || !pictures_drawn(page).is_empty()
            || page_words(page).iter().any(|word| word == "Go to File")
    })?;
    page.advance(0.5)?;
    Ok(page)
}

/// The name of the editor shown (an editor is named by its tab: `doc:3`).
pub(crate) fn shown(page: &Page) -> Option<String> {
    let laid = page.laid()?;
    // The editors of a comparison with the last commit stand beside the file's (`compare:N`): the one
    // shown is the first of a file that is not hidden.
    let rooms = laid.editors();
    (1..500).map(|n| format!("doc:{n}")).find(|name| {
        rooms
            .iter()
            .any(|room| !room.hidden && room.id == Id::named(name))
    })
}

/// The text of the editor shown.
pub(crate) fn text(page: &Page) -> String {
    shown(page)
        .and_then(|name| page.editor_text(&name))
        .unwrap_or_default()
}

/// Whether the page shows `words` (in what it made: a note or a dialog coming in over a moment is
/// shown from its first frame, though it is not yet drawn at full strength).
pub(crate) fn shows(page: &Page, words: &str) -> bool {
    page_words(page).iter().any(|word| word.contains(words))
}

/// The words of the page as it made them, in the order of its tree: each text, and each text of
/// many looks whole.
pub(crate) fn page_words(page: &Page) -> Vec<String> {
    use pencere_view::wire::Kind;
    page.root()
        .map(|root| {
            root.fold()
                .into_iter()
                .filter_map(|node| match node.kind {
                    Kind::Text(words) => Some(words),
                    Kind::Rich(spans) => {
                        Some(spans.iter().map(|span| span.text.as_str()).collect())
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The methods the page called, in order, as the program read them.
pub(crate) fn methods(page: &Page) -> Vec<String> {
    page.sent()
        .iter()
        .filter_map(|said| serde_json::from_str::<Value>(said).ok())
        .filter_map(|message| message["call"]["method"].as_str().map(str::to_owned))
        .collect()
}

/// Waits for the page to come to `done`, the program speaking on its own; where it does not, what
/// it shows is said, with the line of the test that waited.
#[track_caller]
pub(crate) fn wait(page: &mut Page, done: impl FnMut(&Page) -> bool) -> Result<(), Error> {
    let at = std::panic::Location::caller();
    let waited = page.wait_for(WITHIN, done);
    if waited.is_err() {
        eprintln!(
            "{at}: waited in vain; the page shows {:?}, the editor {:?}",
            page_words(page),
            text(page).chars().take(300).collect::<String>()
        );
    }
    waited
}

/// Opens `path` from the quick open, at a line where one is given (`name:12`).
pub(crate) fn go_to(page: &mut Page, path: &str) -> Result<(), Error> {
    page.press("cmd+k")?;
    page.type_text(path)?;
    page.press("enter")?;
    let name = path.split(':').next().unwrap_or(path).to_owned();
    wait(page, |page| {
        page_words(page)
            .iter()
            .any(|word| word.ends_with(name.rsplit('/').next().unwrap_or(&name)))
            && shown(page).is_some()
    })?;
    page.advance(0.5)
}

/// The person clicks the first character of `words` where they are drawn in the element named
/// `within`: a link in a text has no element of its own to be clicked by its name. The page's
/// measure is the drive's own, a character 7 points wide, and a point is a pixel (no `scale`).
fn click_words(page: &mut Page, within: &str, words: &str) -> Result<(), Error> {
    let bounds = page
        .bounds(within)
        .ok_or_else(|| Error::NoSuch(within.to_owned()))?;
    let at = page.list().items.iter().find_map(|item| match item {
        Paint::Text(run) if bounds.contains(run.origin.x, run.origin.y) => {
            run.text.find(words).map(|byte| {
                let before = run.text[..byte].chars().count() as f32;
                Point::new(run.origin.x + before * 7.0 + 3.5, run.origin.y + 8.0)
            })
        }
        _ => None,
    });
    page.click_at(at.ok_or_else(|| Error::NoSuch(words.to_owned()))?)
}

const README: &str = "# Lantern\n\nA small shop API and the worker that sends its mail.\n\n## Run it\n\n```sh\nnpm install\nnpm run dev          # the API on :8080\n```\n\n## What is where\n\n| Path             | What                           |\n| ---------------- | ------------------------------ |\n| `src/server.ts`  | the HTTP server and its routes |\n\n> Prices are kept in cents, never in floats.\n\n- [x] orders\n- [ ] refunds (in progress)\n\nSee [the docs](https://example.com/docs).\n";

const SERVER: &str = "import { createServer } from \"node:http\";\n\nimport { formatPrice } from \"./lib/format.ts\";\n\nconst PORT = Number(process.env.PORT ?? 8080);\n\n/** Answers with JSON. */\nfunction send(res, status: number, body: unknown) {\n  res.end(JSON.stringify(body));\n}\n\nconst server = createServer(async (req, res) => {\n  if (req.method === \"GET\") {\n    return send(res, 200, []);\n  }\n  send(res, 404, { error: `no route for ${req.method}` });\n});\n\nserver.listen(PORT);\n";

/// A small shop's project: what the docs' pictures show.
pub(crate) fn lantern() -> Project {
    Project::new(&[
        ("README.md", README),
        ("src/server.ts", SERVER),
        (
            "src/lib/format.ts",
            "const currency = new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"USD\" });\n\n/** A price in cents, as people read it: 1999 → \"$19.99\". */\nexport function formatPrice(cents: number): string {\n  return currency.format(cents / 100);\n}\n\nexport const TAX_RATE = 0.0825;\n",
        ),
        (
            "src/lib/orders.ts",
            "import { randomUUID } from \"node:crypto\";\n\nexport interface Order {\n  id: string;\n  total: number;\n}\n\nexport class OrderBook {\n  readonly #orders = new Map<string, Order>();\n}\n",
        ),
        (
            "src/routes/refunds.ts",
            "export function refund(orderId: string) {\n  return { orderId, cents: 0 };\n}\n",
        ),
        (
            "worker/mailer.py",
            "#!/usr/bin/env python3\n\"\"\"Sends the receipts.\"\"\"\n\nimport smtplib\n\n\ndef send(order):\n    if order.total > 0:\n        return smtplib.SMTP(\"localhost\")\n    return None\n",
        ),
        (
            "config.yaml",
            "# ports, the database, the mail relay\nport: 8080\ndatabase:\n  url: \"postgres://localhost/lantern\"\n",
        ),
        (
            "Dockerfile",
            "FROM node:22\nWORKDIR /app\nCOPY . .\nRUN npm install\n",
        ),
        ("notes/emoji.txt", "a😀b\nçğı İ ş\n"),
    ])
}

// --- Opening, editing, saving ----------------------------------------------------------------------

/// The file in front is read, typed into in code points, sent a moment after the typing stops, and
/// saved; another tab keeps it with what can be undone.
fn opens_edits_and_saves(size: (f32, f32)) -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts", "README.md"]);
    let mut page = project.open(&session, size)?;
    assert_eq!(text(&page), SERVER);
    assert!(
        shows(&page, "server.ts") && shows(&page, "Ln 1, Col 1"),
        "{:?}",
        page_words(&page)
    );
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("héllo😀 ")?;
    page.advance(1.0)?;
    assert!(
        methods(&page).iter().any(|method| method == "edit"),
        "{:?}",
        methods(&page)
    );
    assert!(shows(&page, "Ln 1, Col 8"), "{:?}", page_words(&page));
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("src/server.ts").starts_with("héllo😀 import")
    })?;
    // Another tab, and back: the text and its history are kept.
    page.press("ctrl+2")?;
    wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
    page.press("ctrl+1")?;
    assert!(text(&page).starts_with("héllo😀 import"));
    page.press("cmd+z")?;
    page.advance(1.0)?;
    assert_eq!(text(&page), SERVER);
    page.press("cmd+s")?;
    wait(&mut page, |_| project.read("src/server.ts") == SERVER)?;
    Ok(())
}

#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_is_typed_into_in_code_points_saved_and_its_undo_kept_across_tabs() -> Result<(), Error> {
    opens_edits_and_saves(WIDE)
}

#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn an_80x24_pane_edits_and_saves_as_a_wide_one_does() -> Result<(), Error> {
    opens_edits_and_saves(PANE)
}

/// A text too long for one message comes in pieces (`partial`, `slice`), the editor holds all of it,
/// and an edit past the first piece reaches the program in code points.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_long_text_comes_in_pieces_and_is_edited_at_its_end() -> Result<(), Error> {
    let mut long = String::new();
    for n in 0..24_000 {
        long.push_str(&format!(
            "line {n} — ünïcödé 😀 a text that goes on for a while\n"
        ));
    }
    assert!(long.len() > 1024 * 1024);
    let project = Project::new(&[("big.txt", &long)]);
    let session = project.session(&["big.txt"]);
    let mut page = project.open(&session, WIDE)?;
    wait(&mut page, |page| text(page).len() == long.len())?;
    assert!(
        methods(&page).iter().any(|method| method == "slice"),
        "{:?}",
        methods(&page)
    );
    assert_eq!(text(&page), long);
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, long.chars().count())?;
    page.type_text("Ω")?;
    page.advance(1.0)?;
    page.press("cmd+s")?;
    wait(&mut page, |_| project.read("big.txt").ends_with("while\nΩ"))?;
    Ok(())
}

/// A paste of 300 000 characters goes to the program in calls of at most 64 Ki characters each; and
/// where the program's text and the editor's part (a call lost), the page reads the program's and
/// sends its own whole.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_long_paste_goes_in_pieces_and_a_text_out_of_step_is_sent_again() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    let paste = "aé😀b\n".repeat(60_000);
    page.insert_text(&paste)?;
    page.advance(1.0)?;
    // What the page said, as the program read it: the calls of `edit`.
    let edits: Vec<Value> = page
        .sent()
        .iter()
        .filter_map(|said| serde_json::from_str::<Value>(said).ok())
        .filter(|message| message["call"]["method"] == "edit")
        .map(|message| message["call"]["params"].clone())
        .collect();
    assert!(edits.len() >= 2, "{} edits", edits.len());
    for params in &edits {
        let inserted: usize = params["edits"]
            .as_array()
            .map(|edits| {
                edits
                    .iter()
                    .map(|edit| edit[2].as_str().map_or(0, |text| text.chars().count()))
                    .sum()
            })
            .unwrap_or(0);
        assert!(inserted <= 64 * 1024, "an edit of {inserted} characters");
    }
    // The program's text moves on without the page: its next edit is out of step.
    let version = version_of(&session, "src/server.ts");
    session
        .buffers()
        .edit("src/server.ts", version, &[(0, 0, "X".to_owned())])
        .expect("an edit of the program's own");
    page.type_text("Z")?;
    page.advance(1.0)?;
    page.press("cmd+s")?;
    let expected = text(&page);
    wait(&mut page, |_| project.read("src/server.ts") == expected)?;
    Ok(())
}

fn version_of(session: &Session, path: &str) -> i64 {
    session
        .buffers()
        .all()
        .into_iter()
        .find(|info| info.path == path)
        .map_or(0, |info| i64::try_from(info.version).unwrap_or(0))
}

// --- Characters of more than one byte ----------------------------------------------------------------
//
// The editor's places are bytes of its text, from 1; the program's are bytes from 0; what a person
// reads (a column, a count of what is selected, a match in a line) is counted in characters. These
// are the crossings, in a text where a character is two, three or four bytes.

const ACCENTS: &str = "çğı İ ş abc\nsecond é😀 line\nlast\n";

/// The program's text of `path`: what it holds, whatever the editor shows.
fn program_text(session: &Session, path: &str) -> String {
    session
        .buffers()
        .text(path)
        .map(|opened| opened.text)
        .unwrap_or_default()
}

/// A column and what is selected are counted in characters, the editor's places being bytes.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_column_and_what_is_selected_are_counted_in_characters_where_the_editor_counts_bytes()
-> Result<(), Error> {
    let project = Project::new(&[("accents.txt", ACCENTS)]);
    let session = project.session(&["accents.txt"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    // After `çğı İ ş`: seven characters, eleven bytes.
    page.caret_to(&editor, 7)?;
    assert!(shows(&page, "Ln 1, Col 8"), "{:?}", page_words(&page));
    // `çğı İ`: five characters, nine bytes.
    page.select(&editor, 0, 5)?;
    assert!(shows(&page, "(5 selected"), "{:?}", page_words(&page));
    // After the emoji on the second line: the line is twelve characters in, nine within the line.
    page.caret_to(&editor, 21)?;
    assert!(shows(&page, "Ln 2, Col 10"), "{:?}", page_words(&page));
    Ok(())
}

/// A line and a column the person names, and a match the search found, are characters of a line: the
/// caret lands after the emoji, not a byte or three bytes before it.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_line_and_a_column_and_a_match_in_a_line_with_wide_characters_are_opened_where_they_are()
-> Result<(), Error> {
    let project = Project::new(&[("accents.txt", ACCENTS)]);
    let session = project.session(&["accents.txt"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let head = |page: &Page| {
        page.editor(&editor)
            .and_then(|buffer| buffer.ranges().first().map(|range| range.head))
    };
    page.press("cmd+l")?;
    page.type_text("2:10")?;
    page.press("enter")?;
    assert!(shows(&page, "Ln 2, Col 10"), "{:?}", page_words(&page));
    assert_eq!(head(&page), Some(21), "the characters before the caret");
    // The project search opens the match, selected, after the emoji.
    page.press("cmd+shift+f")?;
    page.type_text("line")?;
    page.advance(0.5)?;
    wait(&mut page, |page| shows(page, "accents.txt"))?;
    page.press("down")?;
    page.press("enter")?;
    let selected = |page: &Page| {
        let buffer = page.editor(&editor)?;
        let range = buffer.ranges().first()?;
        let text: Vec<char> = buffer.text().chars().collect();
        let (from, to) = (range.anchor.min(range.head), range.anchor.max(range.head));
        Some(text.get(from..to)?.iter().collect::<String>())
    };
    wait(&mut page, |page| selected(page).as_deref() == Some("line"))?;
    Ok(())
}

/// Several carets typing at once make one edit of several places, which the program holds as the
/// editor does, and what a person typed is not lost to a place inside a character.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn an_edit_in_several_places_at_once_is_the_programs_as_it_is_the_editors() -> Result<(), Error> {
    let project = Project::new(&[("accents.txt", "aé😀 aé😀 aé😀\n")]);
    let session = project.session(&["accents.txt"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.select(&editor, 0, 2)?;
    page.press("ctrl+g")?;
    page.press("ctrl+g")?;
    assert_eq!(
        page.editor(&editor).map(|buffer| buffer.ranges().len()),
        Some(3)
    );
    page.type_text("şü")?;
    assert_eq!(text(&page), "şü😀 şü😀 şü😀\n");
    page.advance(1.0)?;
    wait(&mut page, |_| {
        program_text(&session, "accents.txt") == "şü😀 şü😀 şü😀\n"
    })?;
    // Backspace takes a character of two bytes and one of four away at every caret.
    page.press("backspace")?;
    page.advance(1.0)?;
    wait(&mut page, |page| {
        program_text(&session, "accents.txt") == text(page)
    })?;
    page.press("cmd+s")?;
    wait(&mut page, |page| project.read("accents.txt") == text(page))?;
    Ok(())
}

/// Where the caret was is told to the program in bytes (every two seconds, and when the view ends),
/// and comes back, to the next view, at the same character.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_place_kept_for_a_file_comes_back_at_the_same_character() -> Result<(), Error> {
    let project = Project::new(&[("accents.txt", ACCENTS), ("other.txt", "other\n")]);
    let session = project.session(&["accents.txt", "other.txt"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 21)?;
    assert!(shows(&page, "Ln 2, Col 10"), "{:?}", page_words(&page));
    // The page tells the program where it is every two seconds.
    page.advance(3.0)?;
    drop(page);
    // The next view, as after the terminal came back: the caret is where it was, after the emoji.
    let mut again = project.open(&session, WIDE)?;
    wait(&mut again, |page| {
        shown(page).is_some() && text(page) == ACCENTS
    })?;
    again.advance(1.0)?;
    assert!(shows(&again, "Ln 2, Col 10"), "{:?}", page_words(&again));
    let editor = shown(&again).expect("an editor");
    assert_eq!(
        again
            .editor(&editor)
            .and_then(|buffer| buffer.ranges().first().map(|range| range.head)),
        Some(21)
    );
    Ok(())
}

/// A file that changed on disk in a character, where only its last byte differs, is changed there and
/// no byte more, and what was done before can be undone.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_changed_on_disk_inside_characters_of_more_than_one_byte_is_read_again_in_place()
-> Result<(), Error> {
    let project = Project::new(&[("accents.txt", "aé😀b\nçğı\nend\n")]);
    let session = project.session(&["accents.txt"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("X")?;
    page.advance(1.0)?;
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("accents.txt").starts_with("Xaé")
    })?;
    page.advance(1.0)?;
    // `é` and `ê` share their first byte; the emoji's last byte and `ğ`'s differ from their neighbours'.
    let on_disk = "Xaê😁b\nçğı\nend\nüç\n";
    project.write("accents.txt", on_disk);
    wait(&mut page, |page| text(page) == on_disk)?;
    page.advance(0.5)?;
    assert_eq!(shown(&page), Some(editor), "the same editor");
    page.press("cmd+z")?;
    page.advance(0.5)?;
    assert_eq!(text(&page), on_disk.replacen("Xaê", "aê", 1));
    Ok(())
}

/// A file named in characters of more than one byte is found and marked in the quick open and its name
/// is selected, by bytes, in the dialog that renames it.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_name_in_wide_characters_is_marked_in_the_quick_open_and_selected_in_the_rename()
-> Result<(), Error> {
    let project = Project::new(&[("notes/çiçek.txt", "çiçek\n"), ("readme.md", "# Hi\n")]);
    let session = project.session(&["readme.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+k")?;
    page.type_text("çiçek")?;
    wait(&mut page, |page| shows(page, "çiçek.txt"))?;
    page.press("enter")?;
    wait(&mut page, |page| text(page) == "çiçek\n")?;
    // The dialog that renames: `çiçek` (seven bytes, five characters) is selected in `notes/çiçek.txt`;
    // the host's field says it in characters, from the bytes the page gave.
    page.press("cmd+shift+e")?;
    // The tree shows the file opened: its folder is open already.
    wait(&mut page, |page| {
        page.bounds("row:notes/çiçek.txt").is_some()
    })?;
    page.click("row:notes/çiçek.txt")?;
    page.press("f2")?;
    assert_eq!(page.field("dialog-input"), Some("notes/çiçek.txt"));
    assert_eq!(page.field_selection("dialog-input"), Some(6..11));
    Ok(())
}

/// A file with nothing in it, or one that is not there (open empty), opens at the place kept for it and
/// where it is asked to: an empty text into an empty editor is told of by nothing, so the page has no
/// version of that document to say the caret's place against, and the kit refuses an ask without one.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_with_nothing_in_it_opens_where_it_was_left_and_where_it_is_asked_to() -> Result<(), Error>
{
    let project = Project::new(&[("empty.txt", ""), ("other.txt", "other\n")]);
    let session = project.session(&[]);
    let place = |at: u64| Place {
        anchor: serde_json::Number::from(at),
        head: serde_json::Number::from(at),
        top: serde_json::Number::from(0),
    };
    session.keep(UiState {
        tabs: ["empty.txt", "other.txt", "missing.txt"]
            .map(str::to_owned)
            .to_vec(),
        active: Some("empty.txt".to_owned()),
        places: [
            ("empty.txt".to_owned(), place(0)),
            ("missing.txt".to_owned(), place(3)),
            ("other.txt".to_owned(), place(2)),
        ]
        .into(),
        ..UiState::default()
    });
    let mut page = project.open(&session, WIDE)?;
    wait(&mut page, |page| shows(page, "Ln 1, Col 1"))?;
    assert_eq!(text(&page), "");
    // Another program writes to it: the page changes the editor's text, which has had no change yet.
    project.write("empty.txt", "now not empty\n");
    wait(&mut page, |page| text(page) == "now not empty\n")?;
    // The other tabs come to the front one after another, and the empty ones are asked to a place.
    page.press("ctrl+2")?;
    wait(&mut page, |page| text(page) == "other\n")?;
    assert!(shows(&page, "Ln 1, Col 3"), "{:?}", page_words(&page));
    page.press("ctrl+3")?;
    page.advance(0.5)?;
    assert!(shows(&page, "Ln 1, Col 1"), "{:?}", page_words(&page));
    go_to(&mut page, "empty.txt:1:1")?;
    page.press("cmd+l")?;
    page.type_text("1")?;
    page.press("enter")?;
    page.advance(0.5)?;
    assert!(shows(&page, "Ln 1, Col 1"), "{:?}", page_words(&page));
    Ok(())
}

// --- What the program tells of the files -------------------------------------------------------------

/// A file changed on disk with no changes here is read again; one with changes is asked about.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_changed_on_disk_is_read_again_or_asked_about() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    project.write("README.md", "# Lantern\n\nChanged on disk.\n");
    wait(&mut page, |page| {
        text(page) == "# Lantern\n\nChanged on disk.\n"
    })?;
    // A note comes in over a moment (its motion), then says so.
    page.advance(0.5)?;
    assert!(
        shows(&page, "changed on disk and was read again"),
        "{:?}",
        page_words(&page)
    );
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("x")?;
    page.advance(1.0)?;
    project.write("README.md", "# Other\n");
    wait(&mut page, |page| {
        shows(page, "changed on disk while it had changes here")
    })?;
    page.click("banner-conflict-keep")?;
    assert!(!shows(&page, "changed on disk while it had changes here"));
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("README.md").starts_with("x# Lantern")
    })?;
    Ok(())
}

/// What an earlier run left unsaved is offered back, and taken.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_draft_from_the_last_run_is_offered_back_and_taken() -> Result<(), Error> {
    let project = lantern();
    {
        let session = project.session(&["README.md"]);
        let mut page = project.open(&session, WIDE)?;
        let editor = shown(&page).expect("an editor");
        page.caret_to(&editor, 0)?;
        page.type_text("draft ")?;
        page.advance(1.0)?;
        // The program ends with it unsaved (⌃C, a hangup): it is kept as a draft.
        session.ending();
    }
    let session = project.session(&["README.md"]);
    // Seven hours later by the machine's clock: the banner says how long ago the draft was kept.
    let mut page = project.open_later(&session, WIDE, 7.0 * 3600.0)?;
    wait(&mut page, |page| {
        shows(page, "were kept 7 hours ago, when textpen last ended")
    })?;
    assert_eq!(text(&page), README);
    page.click("banner-draft-restore")?;
    wait(&mut page, |page| text(page).starts_with("draft # Lantern"))?;
    page.advance(0.5)?;
    assert!(shows(&page, "are back"), "{:?}", page_words(&page));
    Ok(())
}

/// Closing a tab with changes asks; ⌃Q with changes asks, and quitting without saving ends the view
/// by the program, with no draft kept.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn closing_and_quitting_ask_about_changes_and_quitting_ends_the_view() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts", "README.md"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("x")?;
    page.advance(1.0)?;
    page.press("ctrl+w")?;
    assert!(
        shows(&page, "Save the changes to server.ts?"),
        "{:?}",
        page_words(&page)
    );
    page.press("escape")?;
    assert!(page.bounds("tab:src/server.ts").is_some());
    page.press("ctrl+q")?;
    assert!(shows(&page, "before quitting"), "{:?}", page_words(&page));
    let quit = page.press("q");
    let ended = match quit {
        Err(Error::Closed(_)) => true,
        Ok(_) => matches!(page.advance(1.0), Err(Error::Closed(_))),
        Err(other) => return Err(other),
    };
    assert!(ended, "the program closed its view");
    session.ending();
    assert_eq!(project.read("src/server.ts"), SERVER);
    assert!(
        session
            .buffers()
            .all()
            .iter()
            .all(|info| info.draft.is_none())
    );
    Ok(())
}

// --- The tree, the palette, finding ---------------------------------------------------------------------

/// The editor's caret and selection are what the person's terminal says it draws, and change when
/// the terminal says they changed; a terminal that says nothing leaves the editor's own look.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_editor_looks_as_the_terminal_says_it_draws_its_cursor() -> Result<(), Error> {
    use pencere_view::edit::CaretShape;
    use pencere_view::{TerminalCaret, TerminalLook, TerminalSelection};
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let caret = page.editor(&editor).expect("its buffer").options().caret;
    assert_eq!(
        (caret.shape, caret.radius, caret.glow),
        (CaretShape::Bar, 0.0, 0.0)
    );
    page.set_terminal_look(TerminalLook {
        caret: TerminalCaret {
            shape: Some(CaretShape::Block),
            radius: Some(0.1),
            glow: Some(1.0),
            ..TerminalCaret::default()
        },
        selection: TerminalSelection { radius: Some(0.22) },
    })?;
    let options = page.editor(&editor).expect("its buffer").options();
    assert_eq!(
        (
            options.caret.shape,
            options.caret.radius,
            options.caret.glow
        ),
        (CaretShape::Block, 0.1, 1.0)
    );
    assert_eq!(options.selection.radius, 0.22);
    Ok(())
}

/// Over a row of the tree the pointer is the hand, and it stays the hand when the row is pressed
/// and let go, as it is when the pointer only moves.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_pointer_is_the_hand_over_the_trees_rows_before_and_after_a_click() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.hover("row:src")?;
    assert_eq!(page.cursor(), Some(pencere_view::Cursor::Pointer));
    page.click("row:src")?;
    wait(&mut page, |page| page.bounds("row:src/lib").is_some())?;
    assert_eq!(page.cursor(), Some(pencere_view::Cursor::Pointer));
    page.click("row:README.md")?;
    assert_eq!(page.cursor(), Some(pencere_view::Cursor::Pointer));
    Ok(())
}

/// While the sidebar's edge is dragged the tree makes only the rows in view, where they stood, and
/// a move that does not move the edge draws nothing; let go, the long list makes its rows ahead of
/// a scroll again and the width is kept.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn dragging_the_sidebars_edge_makes_only_the_trees_rows_in_view() -> Result<(), Error> {
    use pencere_view::{Button, Input, Mods};
    let names: Vec<String> = (1..=150).map(|n| format!("f{n:03}.txt")).collect();
    let files: Vec<(&str, &str)> = names.iter().map(|name| (name.as_str(), "")).collect();
    let project = Project::new(&files);
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.wheel("row:f010.txt", 0.0, 400.0)?;
    let seen = "row:f030.txt";
    let ahead = "row:f080.txt";
    let place = |page: &Page, name: &str| page.bounds(name).map(|rect| rect.y);
    let before = place(&page, seen).ok_or_else(|| Error::NoSuch(seen.to_owned()))?;
    assert!(
        place(&page, ahead).is_some(),
        "the list makes rows past the view"
    );
    let edge = page
        .bounds("resizer")
        .ok_or_else(|| Error::NoSuch("resizer".to_owned()))?;
    let at = Point::new(edge.x + edge.w / 2.0, edge.y + edge.h / 2.0);
    page.pointer(Input::Press {
        at,
        button: Button::Left,
        mods: Mods::default(),
        count: 1,
    })?;
    page.pointer(Input::Move(Point::new(at.x + 40.0, at.y)))?;
    assert_eq!(
        page.bounds("resizer").map(|rect| rect.x),
        Some(edge.x + 40.0)
    );
    assert_eq!(
        place(&page, seen),
        Some(before),
        "the rows in view stay where they were"
    );
    assert_eq!(place(&page, ahead), None, "no row past the view is made");
    page.pointer(Input::Move(Point::new(at.x + 40.0, at.y + 30.0)))?;
    let drawn = page.replies().last().map(|answer| answer.reply.redraw);
    assert_eq!(
        drawn,
        Some(false),
        "a move that leaves the edge where it is draws nothing"
    );
    page.pointer(Input::Release {
        at: Point::new(at.x + 40.0, at.y + 30.0),
        button: Button::Left,
    })?;
    assert_eq!(place(&page, seen), Some(before));
    assert!(
        place(&page, ahead).is_some(),
        "let go, the list makes its rows ahead again"
    );
    wait(&mut page, |_| {
        settings_of(&session)["sidebarWidth"] == 240 + 40
    })?;
    Ok(())
}

/// A name longer than the sidebar is wide is shown whole and the tree scrolls across to it; every
/// row is as wide as the widest, so that their grounds line up wherever the tree is scrolled, and
/// the line of a shorter one, what is said of its file at its end, is as wide as the tree.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_tree_narrower_than_its_names_scrolls_across() -> Result<(), Error> {
    let long = "a-name-far-longer-than-a-narrow-sidebar-is-wide-and-then-some.ts";
    let project = Project::new(&[(long, ""), ("b.txt", "")]);
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    assert!(shows(&page, long), "{:?}", page_words(&page));
    let row = |page: &Page, name: &str| {
        page.bounds(&format!("row:{name}"))
            .ok_or_else(|| Error::NoSuch(name.to_owned()))
    };
    let tree = page
        .bounds("tree")
        .ok_or_else(|| Error::NoSuch("tree".to_owned()))?;
    let (wide, short) = (row(&page, long)?, row(&page, "b.txt")?);
    assert!(wide.w > tree.w, "{wide:?} in {tree:?}");
    assert_eq!(short.w, wide.w, "the rows are as wide as the widest");
    let line = page
        .bounds("rowline:b.txt")
        .ok_or_else(|| Error::NoSuch("rowline:b.txt".to_owned()))?;
    assert_eq!(
        line.w, tree.w,
        "a shorter row's line ends at the tree's edge"
    );
    page.wheel_at(
        Point::new(tree.x + 20.0, short.y + short.h / 2.0),
        120.0,
        0.0,
    )?;
    assert_eq!(
        row(&page, "b.txt")?.x,
        short.x - 120.0,
        "the tree scrolled across"
    );
    Ok(())
}

/// The tree opens a folder and a file; a file is made, renamed (its tab and its editor follow) and
/// deleted from its row's menu.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_tree_makes_renames_and_deletes_files() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.click("row:src")?;
    wait(&mut page, |page| page.bounds("row:src/lib").is_some())?;
    page.click("row:src/lib")?;
    wait(&mut page, |page| {
        page.bounds("row:src/lib/format.ts").is_some()
    })?;
    page.click("row:src/lib/format.ts")?;
    wait(&mut page, |page| text(page).starts_with("const currency"))?;
    page.press("a")?;
    assert!(shows(&page, "New file"), "{:?}", page_words(&page));
    page.fill("dialog-input", "src/lib/fresh.ts")?;
    page.press("enter")?;
    wait(&mut page, |_| project.exists("src/lib/fresh.ts"))?;
    wait(&mut page, |page| {
        page.bounds("tab:src/lib/fresh.ts").is_some() && shown(page).is_some()
    })?;
    page.advance(0.5)?;
    let editor = shown(&page).expect("an editor");
    page.type_text("export const fresh = 1;\n")?;
    page.advance(1.0)?;
    page.press("cmd+shift+e")?;
    page.press("r")?;
    assert!(shows(&page, "Rename fresh.ts"), "{:?}", page_words(&page));
    page.fill("dialog-input", "src/lib/ripe.ts")?;
    page.press("enter")?;
    wait(&mut page, |_| {
        project.exists("src/lib/ripe.ts") && !project.exists("src/lib/fresh.ts")
    })?;
    wait(&mut page, |page| {
        page.bounds("tab:src/lib/ripe.ts").is_some()
    })?;
    assert_eq!(shown(&page), Some(editor), "the editor follows its file");
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("src/lib/ripe.ts") == "export const fresh = 1;\n"
    })?;
    wait(&mut page, |page| {
        page.bounds("row:src/lib/ripe.ts").is_some()
    })?;
    page.right_click("row:src/lib/ripe.ts")?;
    page.pick_titled("Delete")?;
    assert!(shows(&page, "Delete ripe.ts?"), "{:?}", page_words(&page));
    page.press("d")?;
    wait(&mut page, |_| !project.exists("src/lib/ripe.ts"))?;
    wait(&mut page, |page| {
        page.bounds("tab:src/lib/ripe.ts").is_none()
    })?;
    Ok(())
}

/// The quick open finds a file among twenty thousand by a few of its letters, typed one at a time,
/// within the call's 250 ms (the page holds its scoring to a chunk a call); `name:line:column` goes
/// there.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_quick_open_finds_a_file_among_twenty_thousand_and_goes_to_a_place() -> Result<(), Error> {
    let project = lantern();
    for n in 0..20_000 {
        project.write(&format!("gen/d{}/s{}/file_{n}.ts", n % 37, n % 11), "");
    }
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+k")?;
    wait(&mut page, |page| shows(page, "Files"))?;
    page.advance(1.0)?;
    page.type_text("file_1999")?;
    wait(&mut page, |page| shows(page, "file_19999.ts"))?;
    page.press("escape")?;
    // A few letters of a name and a place in it.
    page.press("cmd+k")?;
    page.type_text("server:3:5")?;
    page.press("enter")?;
    wait(&mut page, |page| shows(page, "Ln 3, Col 5"))?;
    assert_eq!(text(&page), SERVER);
    Ok(())
}

/// `>` lists the commands, which run; ⌘L goes to a line; ⌘E switches to the file seen before; ⇧⌘K
/// deletes the caret's line; ⌃G adds the next occurrence.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_palette_runs_commands_and_goes_to_lines_and_files() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts", "src/lib/orders.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+k")?;
    page.type_text(">word wrap")?;
    assert!(shows(&page, "Toggle Word Wrap"), "{:?}", page_words(&page));
    page.press("enter")?;
    page.advance(1.0)?;
    assert_eq!(settings_of(&session)["wrap"], true);
    page.press("cmd+l")?;
    page.type_text("12")?;
    page.press("enter")?;
    assert!(shows(&page, "Ln 12, Col 1"), "{:?}", page_words(&page));
    go_to(&mut page, "orders.ts")?;
    page.press("cmd+e")?;
    page.press("enter")?;
    wait(&mut page, |page| text(page) == SERVER)?;
    page.press("cmd+e")?;
    page.press("enter")?;
    wait(&mut page, |page| {
        text(page).starts_with("import { randomUUID")
    })?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.press("cmd+shift+k")?;
    page.advance(1.0)?;
    assert!(
        text(&page).starts_with("\nexport interface Order"),
        "{:?}",
        text(&page)
    );
    page.caret_to(&editor, 3)?;
    page.press("ctrl+g")?;
    page.press("ctrl+g")?;
    assert!(
        page.editor(&editor)
            .map_or(0, |buffer| buffer.ranges().len())
            >= 2
    );
    Ok(())
}

fn settings_of(session: &Session) -> Value {
    serde_json::to_value(session.settings()).unwrap_or_default()
}

/// ⌘F finds as it is typed and counts; ⌥⌘F replaces every match, which the program hears; a pattern
/// that is none says so.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn find_counts_replaces_and_says_when_a_pattern_is_none() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+f")?;
    page.type_text("send")?;
    assert!(shows(&page, "1 of 3"), "{:?}", page_words(&page));
    page.press("enter")?;
    assert!(shows(&page, "2 of 3"), "{:?}", page_words(&page));
    page.press("cmd+alt+f")?;
    page.type_text("reply")?;
    page.press("cmd+enter")?;
    page.advance(1.0)?;
    assert!(!text(&page).contains("send(") && text(&page).contains("reply(res, 404"));
    assert!(shows(&page, "No results"), "{:?}", page_words(&page));
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("src/server.ts").contains("reply(res, 404")
    })?;
    page.press("shift+tab")?;
    page.press("cmd+alt+r")?;
    page.fill("find-query", "(")?;
    assert!(shows(&page, "Not a pattern"), "{:?}", page_words(&page));
    page.press("escape")?;
    assert!(page.bounds("find-bar").is_none());
    Ok(())
}

/// ⇧⌘F searches the project's files, by the program's search; ↓ ⏎ opens a match with it chosen; F4
/// goes on to the next.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_project_is_searched_and_its_matches_opened() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    wait(&mut page, |page| shows(page, "orders.ts"))?;
    // The first match is chosen as the search ends: ↓ goes to it, ↓ to the next file's name and ↓
    // to its first match, which ⏎ opens.
    page.press("down")?;
    let hints = page_words(&page);
    assert!(
        hints.iter().any(|word| word == "show"),
        "the keys of the results: {hints:?}"
    );
    page.press("down")?;
    page.press("down")?;
    page.press("enter")?;
    wait(&mut page, |page| shown(page).is_some())?;
    let editor = shown(&page).expect("an editor");
    let selected = |page: &Page| {
        let buffer = page.editor(&editor)?;
        let range = buffer.ranges().first()?;
        let text: Vec<char> = buffer.text().chars().collect();
        let (from, to) = (range.anchor.min(range.head), range.anchor.max(range.head));
        Some(
            text.get(from..to)?
                .iter()
                .collect::<String>()
                .to_lowercase(),
        )
    };
    wait(&mut page, |page| selected(page).as_deref() == Some("order"))?;
    let before = page
        .editor(&editor)
        .and_then(|buffer| buffer.ranges().first().copied());
    page.press("f4")?;
    page.advance(0.5)?;
    let moved = shown(&page) != Some(editor.clone())
        || page
            .editor(&editor)
            .and_then(|buffer| buffer.ranges().first().copied())
            != before;
    assert!(moved, "F4 went on");
    Ok(())
}

// --- Languages and the preview ----------------------------------------------------------------------

/// A file in each language the page reads, with what is hard for a reader in it.
pub(crate) const SAMPLES: &[(&str, &str, &str)] = &[
    (
        "s/app.js",
        "JavaScript",
        "#!/usr/bin/env node\n// a comment\nconst re = /a[/]b/g; let x = `t ${a + `in ${b}`} end`;\n/* block\n over */ function f(a) { return a.b(1e3, 0x1f) / 2; }\n",
    ),
    (
        "s/view.jsx",
        "JSX",
        "export const V = () => (\n  <div className=\"x\" onClick={() => go(1)}>\n    <Item a='b' />\n  </div>\n);\n",
    ),
    (
        "s/types.ts",
        "TypeScript",
        "interface A<T> { x: Array<string>; y?: number }\nexport type B = keyof A<number>;\n",
    ),
    (
        "s/page.tsx",
        "TSX",
        "const P: FC<Props> = ({ a }) => <Box sx={{ p: 1 }}>{a ? <b>y</b> : null}</Box>;\n",
    ),
    (
        "s/main.rs",
        "Rust",
        "/* outer /* nested */ still */\nfn main<'a>(x: &'a str) {\n    let r = r#\"raw \"quoted\"\n over\"#;\n    println!(\"{}\", 'c');\n}\n",
    ),
    (
        "s/main.go",
        "Go",
        "package main\n\nfunc main() {\n\ts := `raw\nstring`\n\tfmt.Println(s, 'x', 0x10)\n}\n",
    ),
    (
        "s/x.c",
        "C",
        "#include <stdio.h>\nint main(void) { /* c */ char c = '\\n'; return printf(\"%d\", 1); }\n",
    ),
    (
        "s/x.cpp",
        "C++",
        "#include <vector>\nnamespace n { template <typename T> class V { std::vector<T> v; }; }\n",
    ),
    (
        "s/x.cs",
        "C#",
        "public class A { public string S => @\"C:\\path\nnext\"; }\n",
    ),
    (
        "s/A.java",
        "Java",
        "@Override\npublic final class A extends B { String s = \"\"\"\n  text block\n  \"\"\"; }\n",
    ),
    (
        "s/a.kt",
        "Kotlin",
        "data class P(val x: Int) { fun f() = \"\"\"multi\nline\"\"\" }\n",
    ),
    (
        "s/a.swift",
        "Swift",
        "struct S { let x = \"\"\"\n  a\n  \"\"\"; func f() -> Self { self } }\n",
    ),
    (
        "s/i.php",
        "PHP",
        "<?php\n// c\nfunction f($a) { return $this->a . \"s\"; }\n",
    ),
    (
        "s/t.py",
        "Python",
        "@decorator\ndef f(a, b=None):\n    \"\"\"Doc\n    string\"\"\"\n    return f\"{a}\" if a else 0x1F\n",
    ),
    (
        "s/t.rb",
        "Ruby",
        "=begin\nblock\n=end\nclass A\n  def f(x) = :sym && @iv && \"s#{x}\"\nend\n",
    ),
    (
        "s/run",
        "Shell",
        "#!/bin/bash\n# c\nfor f in \"$@\"; do\n  echo \"${f}\" $HOME --flag\ndone\n",
    ),
    (
        "s/t.lua",
        "Lua",
        "--[[ long\ncomment ]]\nlocal s = [==[ long\nstring ]==]\nlocal function f(t) return t.x:y(1) .. \"s\" end\n",
    ),
    (
        "s/q.sql",
        "SQL",
        "-- c\nSELECT a, count(*) FROM t WHERE b = 'x''y' /* c\n */ ORDER BY 1;\n",
    ),
    (
        "s/i.html",
        "HTML",
        "<!doctype html>\n<html><!-- c\n--><script>const a = '<b>'; if (a < 1) {}</script>\n<p class=\"x\">a &amp; b</p></html>\n",
    ),
    (
        "s/d.xml",
        "XML",
        "<?xml version=\"1.0\"?>\n<a b=\"c\"><![CDATA[ x\n y ]]></a>\n",
    ),
    (
        "s/s.css",
        "CSS",
        "/* c */\n.a > #b:hover, div[x] { color: #fff; margin: 0 1.5em !important; }\n",
    ),
    (
        "s/s.scss",
        "SCSS",
        "$c: red;\n.a { &:hover { color: $c; } // c\n}\n",
    ),
    ("s/s.less", "Less", "@c: red;\n.a { color: @c; }\n"),
    (
        "s/r.md",
        "Markdown",
        "# Title\n\nSome **bold** and `code`.\n\n```rust\nfn main() { let x = 1; }\n```\n",
    ),
    (
        "s/c.yaml",
        "YAML",
        "# c\nkey: value # trailing\nblock: |\n  text\nanchor: &x 1\n",
    ),
    (
        "s/c.toml",
        "TOML",
        "[table]\nkey = \"v\"\nm = \"\"\"\nmulti\n\"\"\"\n",
    ),
    (
        "s/c.json",
        "JSON",
        "{\n  \"a\": [1, 2.5e3, true, null]\n}\n",
    ),
    ("s/c.ini", "INI", "; c\n[section]\nkey = value\n"),
    (
        "s/Dockerfile",
        "Dockerfile",
        "FROM node:22 AS build\n# c\nRUN npm ci\n",
    ),
    (
        "s/Makefile",
        "Makefile",
        "# c\nCC := cc\nall: build\n\tcc -o $@ $^\n",
    ),
    (
        "s/x.diff",
        "Diff",
        "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n",
    ),
];

/// Each language colours its sample, and says its name on the status line; ⌘/ comments with the
/// language's own comment, and a line break after a colon indents (Python's rule).
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn every_language_colours_its_sample_and_comments_and_indents_as_it_says() -> Result<(), Error> {
    let project = lantern();
    for (path, _, sample) in SAMPLES {
        project.write(path, sample);
    }
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    for (path, language, sample) in SAMPLES {
        go_to(&mut page, &format!("{path}:1"))?;
        assert_eq!(text(&page), *sample, "{path}");
        assert!(
            page_words(&page).iter().any(|word| word == language),
            "{path}: not {language}: {:?}",
            page_words(&page)
        );
        let editor = shown(&page).expect("an editor");
        let coloured = page.editor(&editor).map_or(0, |buffer| {
            buffer.decorations_in(0, sample.chars().count()).len()
        });
        assert!(coloured > 0, "{path}: nothing coloured");
    }
    go_to(&mut page, "s/t.py:2")?;
    page.press("cmd+/")?;
    page.advance(1.0)?;
    assert!(
        text(&page).contains("# def f(a, b=None):"),
        "{}",
        text(&page)
    );
    page.press("cmd+/")?;
    let editor = shown(&page).expect("an editor");
    let colon = text(&page)
        .find(":\n")
        .map(|at| text(&page)[..=at].chars().count());
    page.caret_to(&editor, colon.unwrap_or(0))?;
    page.press("enter")?;
    page.type_text("pass")?;
    page.advance(1.0)?;
    assert!(
        text(&page).contains("None):\n  pass\n"),
        "{:?}",
        text(&page)
    );
    // The languages of the kit's library are all there to choose from, and a chosen one is kept.
    page.command("language")?;
    assert!(
        shows(&page, "PostgreSQL") && shows(&page, "SQLite") && shows(&page, "Plain Text"),
        "{:?}",
        page_words(&page)
    );
    page.type_text("sqlite")?;
    page.press("enter")?;
    assert!(
        page_words(&page).iter().any(|word| word == "SQLite"),
        "{:?}",
        page_words(&page)
    );
    Ok(())
}

/// ⇧⌘V shows a Markdown file as it reads: headings, code, a table, a quote, tasks; a link opens in
/// the person's browser through the terminal.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_markdown_files_preview_reads_as_it_should_and_opens_its_links() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+v")?;
    assert!(page.bounds("preview").is_some(), "{:?}", page_words(&page));
    for words in [
        "Lantern",
        "What is where",
        "the HTTP server and its routes",
        "Prices are kept in cents",
        "refunds (in progress)",
        "npm install",
    ] {
        assert!(shows(&page, words), "{words}: {:?}", page_words(&page));
    }
    click_words(&mut page, "preview", "the docs")?;
    assert_eq!(page.links(), ["https://example.com/docs"]);
    // Typed into, a long file's preview keeps each call within its time.
    let mut long = String::new();
    for n in 0..4000 {
        long.push_str(&format!("## Section {n}\n\nSome **bold** text with `code` and a [link](https://x.y/{n}).\n\n- [x] task\n\n"));
    }
    project.write("long.md", &long);
    go_to(&mut page, "long.md")?;
    page.press("cmd+shift+v")?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("abc")?;
    page.advance(1.0)?;
    assert!(shows(&page, "Section 3"), "{:?}", page_words(&page));
    Ok(())
}

// --- Pictures -----------------------------------------------------------------------------------------

/// A PNG of `width` by `height` pixels, each pixel what `pixel(x, y)` says (RGB). Its data is stored,
/// not compressed: a file as large as the pixels are, which is how a test makes one that is too large.
pub(crate) fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFF_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = name.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            raw.extend(pixel(x, y));
        }
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        zlib.extend((block.len() as u16).to_le_bytes());
        zlib.extend((!(block.len() as u16)).to_le_bytes());
        zlib.extend(*block);
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A picture with a blue-to-green slope and a white frame, so that its being whole is seen.
pub(crate) fn sloped(width: u32, height: u32) -> Vec<u8> {
    png(width, height, |x, y| {
        if x < 3 || y < 3 || x + 3 >= width || y + 3 >= height {
            return [250, 250, 250];
        }
        let across = (x * 255 / width) as u8;
        let down = (y * 255 / height) as u8;
        [40, 90 + down / 2, 255 - across]
    })
}

/// The pictures the page draws that the program attached, with where each is drawn (pixels).
pub(crate) fn pictures_drawn(page: &Page) -> Vec<(String, pencere_view::Rect)> {
    page.list()
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Image(picture) if picture.src.starts_with("sha256:") => {
                Some((picture.src.clone(), picture.rect))
            }
            _ => None,
        })
        .collect()
}

/// The program's side of the terminal that holds what is attached, a recorder; the host with no
/// screen is given the same files beforehand, so that it has what the page names.
pub(crate) fn recording(session: &Session) -> Arc<Recorder> {
    let recorder = Arc::new(Recorder::default());
    session
        .pictures
        .attaching(Arc::clone(&recorder) as Arc<dyn crate::state::pictures::Attach>);
    recorder
}

/// How the page says the size of a file of `bytes`, with its pixels.
fn caption(bytes: &[u8], pixels: &str) -> String {
    let size = bytes.len() as f64;
    let words = if size < 1024.0 * 1024.0 {
        format!("{:.1} KB", size / 1024.0)
    } else {
        format!("{:.1} MB", size / 1024.0 / 1024.0)
    };
    format!("{words} · {pixels}")
}

/// The files of a project with pictures in it: `logo.png` of 200 × 100, a `photo.png` that does not fit
/// the page as it is, a drawing, and a Markdown file that shows some of them.
fn with_pictures() -> (Project, Vec<(&'static str, Vec<u8>)>) {
    let project = lantern();
    let logo = sloped(200, 100);
    let photo = sloped(1600, 1200);
    let drawing = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"120\" height=\"60\"><rect width=\"120\" height=\"60\" fill=\"#c33\"/></svg>".to_vec();
    project.write_bytes("logo.png", &logo);
    project.write_bytes("photo.png", &photo);
    project.write_bytes("art/mark.svg", &drawing);
    (
        project,
        vec![
            ("logo.png", logo),
            ("photo.png", photo),
            ("art/mark.svg", drawing),
        ],
    )
}

/// The picture in front of the page: where it is drawn, once it is.
fn picture_in_front(page: &mut Page, bytes: &[u8]) -> Result<(String, pencere_view::Rect), Error> {
    let name = format!("sha256:{}", {
        use sha2::{Digest as _, Sha256};
        format!("{:x}", Sha256::digest(bytes))
    });
    wait(page, |page| {
        pictures_drawn(page).iter().any(|(src, _)| *src == name)
    })?;
    page.advance(0.3)?;
    let drawn = pictures_drawn(page);
    drawn
        .into_iter()
        .find(|(src, _)| *src == name)
        .ok_or(Error::NoSuch(name))
}

/// A picture opens in its tab, fitted into it and not made larger, with its size and its pixels under
/// it; it is let go when its tab closes.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_picture_opens_in_its_tab_fitted_with_its_size_under_it_and_is_let_go_when_it_closes()
-> Result<(), Error> {
    let (project, files) = with_pictures();
    let session = project.session(&["README.md"]);
    let recorder = recording(&session);
    let mut page = project.open(&session, WIDE)?;
    for (_, bytes) in &files {
        page.attach(bytes);
    }
    let bytes = |name: &str| {
        files
            .iter()
            .find(|file| file.0 == name)
            .expect("a file")
            .1
            .clone()
    };

    // Small, it is drawn as it is.
    page.click("row:logo.png")?;
    let (name, rect) = picture_in_front(&mut page, &bytes("logo.png"))?;
    assert!(
        (rect.w - 200.0).abs() < 1.0 && (rect.h - 100.0).abs() < 1.0,
        "not made larger: {rect:?}"
    );
    assert!(
        shows(&page, &caption(&bytes("logo.png"), "200 × 100")),
        "{:?}",
        page_words(&page)
    );
    assert_eq!(recorder.attached(), std::slice::from_ref(&name));
    assert!(bounds_of(&page, "tab:logo.png"), "its tab");

    // Large, it is made smaller to fit the room the tab has, in its proportions, and says its own size.
    page.click("row:photo.png")?;
    let (_, rect) = picture_in_front(&mut page, &bytes("photo.png"))?;
    let (w, h) = (rect.w, rect.h);
    assert!(w < 1600.0 && h < 1200.0, "made smaller: {rect:?}");
    assert!(
        (w / h - 4.0 / 3.0).abs() < 0.02,
        "in its proportions: {rect:?}"
    );
    let room = page.bounds("tab:photo.png").expect("the tab").y + 36.0;
    assert!(
        rect.y >= room - 1.0 && rect.y + h <= WIDE.1 - 24.0 && rect.x + w <= WIDE.0,
        "inside the page, between the tabs and the status line: {rect:?}"
    );
    assert!(
        shows(&page, &caption(&bytes("photo.png"), "1600 × 1200")),
        "{:?}",
        page_words(&page)
    );

    // A drawing too.
    page.click("row:art")?;
    wait(&mut page, |page| page.bounds("row:art/mark.svg").is_some())?;
    page.click("row:art/mark.svg")?;
    let (_, rect) = picture_in_front(&mut page, &bytes("art/mark.svg"))?;
    assert!(
        (rect.w - 120.0).abs() < 1.0 && (rect.h - 60.0).abs() < 1.0,
        "{rect:?}"
    );
    assert!(shows(&page, "120 × 60"), "{:?}", page_words(&page));

    // Back to the first: nothing is let go for a tab that is only not in front.
    page.click("tab:logo.png")?;
    picture_in_front(&mut page, &bytes("logo.png"))?;
    assert!(recorder.detached().is_empty(), "{:?}", recorder.detached());

    // Closing the tab lets its picture go, and only its.
    page.press("ctrl+w")?;
    wait(&mut page, |page| page.bounds("tab:logo.png").is_none())?;
    wait(&mut page, |_| recorder.detached() == [name.clone()])?;
    assert!(
        methods(&page).contains(&"release".to_owned()),
        "{:?}",
        methods(&page)
    );
    Ok(())
}

fn bounds_of(page: &Page, name: &str) -> bool {
    page.bounds(name).is_some()
}

/// In an 80×24 pane the picture is fitted to what is left of it, and the sidebar is not there.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_picture_is_fitted_into_a_small_pane() -> Result<(), Error> {
    let (project, files) = with_pictures();
    let session = project.session(&["photo.png"]);
    recording(&session);
    let mut page = project.open_holding(&session, PANE, &[&files[1].1])?;
    let (_, rect) = picture_in_front(&mut page, &files[1].1)?;
    assert!(rect.w < 600.0 && rect.h < 330.0 - 36.0 - 24.0, "{rect:?}");
    assert!((rect.w / rect.h - 4.0 / 3.0).abs() < 0.02, "{rect:?}");
    assert!(shows(&page, "1600 × 1200"), "{:?}", page_words(&page));
    Ok(())
}

/// A picture past 4096 pixels either way, or past 8 MiB, is not shown: the tab says so, with the
/// file's size, and offers to close; nothing is attached.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_picture_too_large_to_preview_says_so_in_its_tabs_place() -> Result<(), Error> {
    let project = lantern();
    project.write_bytes("wide.png", &png(4097, 4, |_, _| [9, 9, 9]));
    project.write_bytes("tall.png", &png(4, 4097, |_, _| [9, 9, 9]));
    project.write_bytes("heavy.png", &png(1700, 1700, |x, y| [(x ^ y) as u8, 0, 0]));
    assert!(
        std::fs::metadata(project.root().join("heavy.png"))
            .expect("a file")
            .len()
            > 8 * 1024 * 1024
    );
    let session = project.session(&["README.md"]);
    let recorder = recording(&session);
    let mut page = project.open(&session, WIDE)?;
    for (file, size) in [("wide.png", "KB"), ("tall.png", "KB"), ("heavy.png", "MB")] {
        page.click(&format!("row:{file}"))?;
        wait(&mut page, |page| shows(page, "Too large to preview"))?;
        let words = page_words(&page);
        assert!(
            words
                .iter()
                .any(|word| word.starts_with("Too large to preview · ") && word.ends_with(size)),
            "{file}: {words:?}"
        );
        assert!(shows(&page, "Close Tab"), "{file}: {words:?}");
        assert!(pictures_drawn(&page).is_empty(), "{file}");
        page.click("placeholder-close")?;
        wait(&mut page, |page| {
            page.bounds(&format!("tab:{file}")).is_none()
        })?;
    }
    assert!(recorder.attached().is_empty(), "{:?}", recorder.attached());
    Ok(())
}

/// A file named like a picture that is none opens as it did: as text where it is text, as a file that
/// is not where it is not; and other pictures than the three are the placeholder they were.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_file_named_like_a_picture_that_is_none_opens_as_it_did() -> Result<(), Error> {
    let project = lantern();
    project.write("words.png", "this is text");
    project.write_bytes("noise.jpg", &[0xFF, 0x00, 0x01, 0x02, 0x00]);
    project.write_bytes("anim.gif", b"GIF89a\x01\0\x01\0\0\0\0;\0");
    project.write_bytes("broken.svg", b"<svg><g/></svg>");
    let session = project.session(&["README.md"]);
    let recorder = recording(&session);
    let mut page = project.open(&session, WIDE)?;
    page.click("row:words.png")?;
    wait(&mut page, |page| text(page) == "this is text")?;
    page.click("row:noise.jpg")?;
    wait(&mut page, |page| shows(page, "noise.jpg is not text"))?;
    page.click("row:anim.gif")?;
    wait(&mut page, |page| shows(page, "anim.gif is not text"))?;
    // A drawing that says no size is text to read.
    page.click("row:broken.svg")?;
    wait(&mut page, |page| text(page) == "<svg><g/></svg>")?;
    assert!(recorder.attached().is_empty());
    // The page asked about the three that are named like pictures, and not about the fourth.
    assert_eq!(
        methods(&page)
            .iter()
            .filter(|method| *method == "picture")
            .count(),
        3,
        "{:?}",
        methods(&page)
    );
    Ok(())
}

/// A page that starts on a picture tab draws the picture, as it was left.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_page_that_starts_on_a_picture_draws_it() -> Result<(), Error> {
    let (project, files) = with_pictures();
    let session = project.session(&["logo.png", "README.md"]);
    recording(&session);
    let mut page = project.open_holding(&session, WIDE, &[&files[0].1])?;
    picture_in_front(&mut page, &files[0].1)?;
    assert!(shows(&page, "200 × 100"), "{:?}", page_words(&page));
    Ok(())
}

/// A Markdown file's preview draws the pictures it names that are in the project, beside the file;
/// the ones that are not there, are out of the project or are addresses stay the words they were.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_markdown_preview_draws_the_pictures_it_names_and_leaves_the_others_as_words()
-> Result<(), Error> {
    let project = lantern();
    let logo = sloped(160, 90);
    project.write_bytes("docs/images/logo.png", &logo);
    // Beside the project, not in it.
    std::fs::write(project.root().with_file_name("outside.png"), sloped(30, 30)).expect("a file");
    project.write(
        "docs/guide.md",
        "# Guide\n\n![Our logo](images/logo.png)\n\n![Not there](images/gone.png)\n\n![Out of reach](../../outside.png)\n\n![On the web](https://example.com/x.png)\n\n![Absolute](/etc/x.png)\n",
    );
    let session = project.session(&["docs/guide.md"]);
    let recorder = recording(&session);
    let mut page = project.open(&session, WIDE)?;
    page.attach(&logo);
    page.press("cmd+shift+v")?;
    let (name, rect) = picture_in_front(&mut page, &logo)?;
    assert!(
        rect.w > 100.0 && (rect.w / rect.h - 160.0 / 90.0).abs() < 0.02,
        "{rect:?}"
    );
    for words in ["Not there", "Out of reach", "On the web", "Absolute"] {
        assert!(shows(&page, words), "{words}: {:?}", page_words(&page));
    }
    assert!(!shows(&page, "Our logo"), "{:?}", page_words(&page));
    assert_eq!(pictures_drawn(&page).len(), 1);
    // Only the picture in the project was given to the terminal.
    assert_eq!(recorder.attached(), std::slice::from_ref(&name));
    // Typing goes on beside it; the picture stays.
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("x")?;
    page.advance(1.0)?;
    assert_eq!(pictures_drawn(&page).len(), 1);
    // It is let go with the file's tab.
    page.press("cmd+shift+v")?;
    page.press("ctrl+w")?;
    wait(&mut page, |page| {
        page.bounds("tab:docs/guide.md").is_none() || shows(page, "Save the changes")
    })?;
    if shows(&page, "Save the changes") {
        page.press("d")?;
    }
    wait(&mut page, |_| recorder.detached() == [name.clone()])?;
    Ok(())
}

// --- The pane, the settings, the start --------------------------------------------------------------

/// The shell button hands the pane to a shell in the project; when it ends, the page comes back where
/// it was and says so.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_pane_is_handed_to_a_shell_and_the_page_comes_back_saying_so() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let handed = page.press("cmd+j").and_then(|_| page.advance(1.0));
    assert!(matches!(handed, Err(Error::Closed(_))), "{handed:?}");
    let mut program = page.into_peer();
    let shell = program
        .lent()
        .map(|command| command.get_current_dir().map(Path::to_path_buf));
    assert_eq!(shell, Some(Some(project.root())), "a shell in the project");
    assert!(program.give_back(ExitStatus::from_raw(0)));
    let mut page = open_with(program, WIDE, None)?;
    wait(&mut page, |page| shows(page, "Back from the shell"))?;
    wait(&mut page, |page| text(page) == SERVER)?;
    Ok(())
}

/// ⌘, shows the settings, each changed with Space and kept by the program; ⇧⌘/ shows the keys,
/// filtered as they are typed.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_settings_are_changed_and_kept_and_the_keys_are_listed() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+,")?;
    assert!(shows(&page, "Show whitespace"), "{:?}", page_words(&page));
    for _ in 0..4 {
        page.press("down")?;
    }
    page.press("space")?;
    page.press("escape")?;
    page.advance(1.0)?;
    assert_eq!(settings_of(&session)["whitespace"], "boundary");
    page.press("cmd+shift+/")?;
    assert!(shows(&page, "Save All"), "{:?}", page_words(&page));
    page.type_text("close tab")?;
    assert!(
        shows(&page, "Close Tab") && !shows(&page, "Save All"),
        "{:?}",
        page_words(&page)
    );
    Ok(())
}

/// With no file open the page says where the project is and what to do first; a click on one goes.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn with_no_file_open_the_page_starts_with_the_project_and_what_to_do() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let mut page = project.open(&session, PANE)?;
    assert!(
        shows(&page, "lantern") && shows(&page, "Go to File"),
        "{:?}",
        page_words(&page)
    );
    assert!(
        !page.outline().tips.is_empty(),
        "the icons say what they do"
    );
    page.click("welcome:files")?;
    assert!(page.bounds("palette-box").is_some());
    Ok(())
}

// --- A dialog is answered once, whatever takes it away ------------------------------------------------

/// The page as it is, under a script that asks a question with the page's own dialog when it is
/// told `probe-ask`, and counts the answers its dialog gives (`probe <count> <last answer>`).
const PROBE: &str = r#"
local page = require("./page")
local kit = require("./lib/kit")
local count, said = 0, "none"
return {
	render = function(size, now, reduce)
		local root, animating, wake = page.render(size, now, reduce)
		root:command("probe-ask", "Probe: ask", nil)
		root:child(pencere.text("probe " .. count .. " " .. said))
		return root, animating, wake
	end,
	settle = page.settle,
	handle = function(event)
		if event.type == "command" and event.id == "probe-ask" then
			kit.act.ask({
				title = "Probe?",
				actions = { { id = "yes", label = "Yes", primary = true }, { id = "no", label = "No" } },
			}, function(action)
				count += 1
				said = tostring(action)
			end)
			return kit.take(true)
		end
		return page.handle(event)
	end,
}
"#;

/// The app's bundle with the probe beside the page.
struct WithProbe(pencere_drive::headless::Files);

impl pencere_sandbox::Assets for WithProbe {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        if path == "probe.lua" {
            return Some(PROBE.as_bytes().into());
        }
        self.0.read(path)
    }

    fn media_type(&self, path: &str) -> Option<String> {
        if path == "probe.lua" {
            return Some("text/x-lua".to_owned());
        }
        self.0.media_type(path)
    }
}

fn probe_says(page: &Page) -> String {
    page_words(page)
        .into_iter()
        .find(|word| word.starts_with("probe "))
        .unwrap_or_default()
}

/// A command asked for over a dialog (⌘K), another dialog asked in its place, and a button: each
/// answers the dialog exactly once — nil for the first two — and the command still runs.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_dialog_is_answered_exactly_once_however_it_goes() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["README.md"]);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let files = WithProbe(pencere_drive::headless::Files::app(env!(
        "CARGO_MANIFEST_DIR"
    )));
    let mut page = Headless::bundle(files, "probe.lua")
        .size(WIDE.0, WIDE.1)
        .peer(program)
        .open()?;
    wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
    assert_eq!(probe_says(&page), "probe 0 none");
    // A command over the dialog: it is answered nil, and the command runs.
    page.command("probe-ask")?;
    assert!(shows(&page, "Probe?"), "{:?}", page_words(&page));
    page.press("cmd+k")?;
    assert_eq!(probe_says(&page), "probe 1 nil");
    assert!(page.bounds("palette-box").is_some(), "the command ran");
    page.press("escape")?;
    // A dialog in the place of another: the first is answered nil.
    page.command("probe-ask")?;
    page.command("probe-ask")?;
    assert_eq!(probe_says(&page), "probe 2 nil");
    // Its own button: the answer, once.
    page.press("y")?;
    assert_eq!(probe_says(&page), "probe 3 yes");
    page.advance(1.0)?;
    assert_eq!(probe_says(&page), "probe 3 yes");
    Ok(())
}

// --- What the kit's newer calls give the page ---------------------------------------------------------

/// The field of the host's named `name` has the keyboard.
fn keyboard_is_in(page: &Page, name: &str) -> bool {
    page.focus() == Focus::Field(Id::named(name))
}

/// Every field the page opens with a text and the keyboard has both the moment it is there: the
/// find bar, the palette, a dialog and the sheet of keys. What is seeded is selected, so that what is
/// typed takes its place; a file's name is selected in the dialog that renames it, not its folders
/// or its extension.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_field_opens_with_its_text_selected_and_the_keyboard_in_it() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    // The word the caret is selected on is what the find bar is opened with, selected.
    let editor = shown(&page).expect("an editor");
    page.select(&editor, 0, 6)?;
    page.press("cmd+f")?;
    assert!(keyboard_is_in(&page, "find-query"), "{:?}", page.focus());
    assert_eq!(page.field("find-query"), Some("import"));
    assert_eq!(page.field_selection("find-query"), Some(0..6));
    page.type_text("send")?;
    assert_eq!(page.field("find-query"), Some("send"), "typed over");
    page.press("escape")?;
    // The palette: opened with `>` and a query at once, the query is selected.
    page.press("cmd+k")?;
    assert!(keyboard_is_in(&page, "palette-query"), "{:?}", page.focus());
    page.fill("palette-query", ">word wrap")?;
    assert_eq!(page.field("palette-query"), Some("word wrap"));
    assert_eq!(page.field_selection("palette-query"), Some(0..9));
    assert!(keyboard_is_in(&page, "palette-query"), "{:?}", page.focus());
    page.type_text("save")?;
    assert_eq!(page.field("palette-query"), Some("save"), "typed over");
    page.press("escape")?;
    // The dialog that renames: the name is selected, in the path of the file.
    page.press("cmd+shift+e")?;
    wait(&mut page, |page| page.bounds("row:src/server.ts").is_some())?;
    page.click("row:src/server.ts")?;
    page.press("f2")?;
    assert!(keyboard_is_in(&page, "dialog-input"), "{:?}", page.focus());
    assert_eq!(page.field("dialog-input"), Some("src/server.ts"));
    assert_eq!(page.field_selection("dialog-input"), Some(4..10));
    page.press("escape")?;
    // The sheet of keys.
    page.press("cmd+shift+/")?;
    assert!(keyboard_is_in(&page, "help-filter"), "{:?}", page.focus());
    Ok(())
}

/// Dragged as narrow as it goes, the sidebar still has every button of its head, which a press reaches;
/// and the search's field keeps its toggles inside it however narrow the sidebar is.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_sidebar_at_its_narrowest_keeps_its_buttons_and_the_search_its_toggles() -> Result<(), Error>
{
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    let edge = page
        .bounds("resizer")
        .ok_or_else(|| Error::NoSuch("resizer".to_owned()))?;
    let at = edge.y + edge.h / 2.0;
    page.drag_between(Point::new(edge.x + edge.w / 2.0, at), Point::new(1.0, at))?;
    let panel = page.bounds("sidebar").expect("the sidebar");
    assert!(panel.w >= 228.0, "{panel:?}");
    assert!(
        !shows(&page, "selected"),
        "the drag selected text: {:?}",
        page_words(&page)
    );
    let right = panel.x + panel.w;
    page.press("cmd+shift+e")?;
    for button in [
        "sidebar-new-file",
        "sidebar-new-folder",
        "sidebar-collapse",
        "sidebar-hide",
    ] {
        let rect = page.bounds(button).expect(button);
        assert!(rect.x + rect.w <= right, "{button} {rect:?} past {right}");
    }
    page.press("cmd+shift+f")?;
    let toggle = page.bounds("search-regex").expect("the toggle");
    assert!(
        toggle.x + toggle.w <= right - 10.0,
        "{toggle:?} past {right}"
    );
    page.click("sidebar-hide")?;
    assert!(page.bounds("sidebar").is_none(), "the last button reached");
    Ok(())
}

/// What is said under the pointer over a palette, a dialog and the sheet of keys: the hand on what a press
/// does something on, the I-beam on the field and the arrow on the ground, which is no button.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_pointer_over_what_is_over_the_page_says_what_is_under_it() -> Result<(), Error> {
    use pencere_view::Cursor;
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    let says = |page: &mut Page, at: Point| -> Result<Option<Cursor>, Error> {
        page.move_to(at)?;
        Ok(page.cursor())
    };
    let middle =
        |rect: pencere_view::Rect| Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    // The palette.
    page.press("cmd+k")?;
    page.settle()?;
    let card = page.bounds("palette-box").expect("the card");
    let field = page.bounds("palette-query").expect("the field");
    let item = page.bounds("palette-item:1").expect("a row");
    assert_eq!(
        says(&mut page, Point::new(4.0, WIDE.1 - 40.0))?,
        Some(Cursor::Arrow),
        "the ground"
    );
    assert_eq!(
        says(&mut page, Point::new(card.x + 2.0, card.y + card.h - 2.0))?,
        Some(Cursor::Arrow),
        "the card"
    );
    assert_eq!(
        says(&mut page, middle(field))?,
        Some(Cursor::Text),
        "the field"
    );
    assert_eq!(
        says(&mut page, middle(item))?,
        Some(Cursor::Pointer),
        "a row"
    );
    page.press("escape")?;
    // A dialog: the same, and its field stands in from the edge of its box.
    page.press("cmd+alt+n")?;
    page.settle()?;
    let dialog = page.bounds("dialog-box").expect("the dialog");
    let field = page.bounds("dialog-input").expect("the field");
    let boxed = page.bounds("dialog-input-box").expect("the field's box");
    assert!(field.x >= boxed.x + 8.0, "{field:?} in {boxed:?}");
    assert!(
        field.x + field.w <= boxed.x + boxed.w - 8.0,
        "{field:?} in {boxed:?}"
    );
    // A press in the room around the words leaves the keyboard in the field.
    page.click_at(Point::new(boxed.x + 3.0, boxed.y + boxed.h / 2.0))?;
    assert!(keyboard_is_in(&page, "dialog-input"), "{:?}", page.focus());
    let action = page.bounds("dialog-action:1").expect("a button");
    assert_eq!(
        says(&mut page, Point::new(4.0, WIDE.1 - 40.0))?,
        Some(Cursor::Arrow),
        "the ground"
    );
    assert_eq!(
        says(&mut page, Point::new(dialog.x + 4.0, dialog.y + 4.0))?,
        Some(Cursor::Arrow),
        "the box"
    );
    assert_eq!(
        says(&mut page, middle(field))?,
        Some(Cursor::Text),
        "the field"
    );
    assert_eq!(
        says(&mut page, middle(action))?,
        Some(Cursor::Pointer),
        "a button"
    );
    page.press("escape")?;
    // The sheet of keys.
    page.press("cmd+shift+/")?;
    page.settle()?;
    let sheet = page.bounds("sheet-box").expect("the sheet");
    let field = page.bounds("help-filter").expect("the field");
    let boxed = page.bounds("help-filter-box").expect("the field's box");
    assert!(field.x >= boxed.x + 8.0, "{field:?} in {boxed:?}");
    assert_eq!(
        says(&mut page, Point::new(4.0, WIDE.1 - 40.0))?,
        Some(Cursor::Arrow),
        "the ground"
    );
    assert_eq!(
        says(
            &mut page,
            Point::new(sheet.x + 40.0, sheet.y + sheet.h - 4.0)
        )?,
        Some(Cursor::Arrow),
        "the sheet"
    );
    assert_eq!(
        says(&mut page, middle(field))?,
        Some(Cursor::Text),
        "the field"
    );
    Ok(())
}

/// The palette's field is never said to be covered as the palette slides in, however its boxes fall on
/// the pixels: a terminal hides a field the page's boxes cover, and the keyboard in it is then gone
/// with no key reaching anything. Sizes and zooms where the field touched the line under it.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_palette_keeps_its_field_uncovered_on_its_way_in() -> Result<(), Error> {
    let project = lantern();
    for ((width, height), zoom) in [
        ((884.0, 520.0), 1.04),
        ((1000.0, 700.0), 1.12),
        ((1280.0, 800.0), 1.2),
        ((1437.0, 871.0), 1.28),
    ] {
        let session = project.session(&["src/server.ts"]);
        let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
        let mut page = Headless::app(env!("CARGO_MANIFEST_DIR"))
            .size(width, height)
            .scale(2.0)
            .limits(limits(None))
            .peer(program)
            .open()?;
        page.wait_for(WITHIN, |page| shown(page).is_some())?;
        page.zoom(zoom)?;
        page.settle()?;
        page.press("cmd+k")?;
        assert!(keyboard_is_in(&page, "palette-query"), "{:?}", page.focus());
        for step in 0..30 {
            page.advance(1.0 / 240.0)?;
            let laid = page.laid().expect("the page laid out");
            let field = laid
                .fields()
                .into_iter()
                .find(|field| field.id == Id::named("palette-query"))
                .expect("the field");
            assert!(!field.covered, "{width}x{height} at {zoom}, step {step}");
        }
    }
    Ok(())
}

/// The colour the page writes the name `word` in.
fn ink_of(page: &Page, word: &str) -> String {
    use pencere_view::wire::Kind;
    let root = page.root().expect("a page");
    let node = root
        .fold()
        .into_iter()
        .find(|node| matches!(&node.kind, Kind::Text(words) if words == word))
        .unwrap_or_else(|| panic!("{word} is not on the page: {:?}", page_words(page)));
    format!("{:?}", node.style.text.color)
}

/// What git ignores is quiet in the tree, and all that is in a folder it ignores: not only the folder.
/// Nothing git keeps is quiet for being in a folder, whatever that folder is called.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn what_git_ignores_is_quiet_all_the_way_down_in_the_tree() -> Result<(), Error> {
    let project = lantern();
    project.write(".gitignore", "out/\n*.log\n");
    project.write("out/deep/inner.txt", "x\n");
    project.write("logs/today.log", "x\n");
    project.write("logs/kept.txt", "x\n");
    project.write("build/source.txt", "x\n");
    crate::page_changes::committed(&project);
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+e")?;
    for dir in ["out", "out/deep", "logs", "build"] {
        wait(&mut page, |page| {
            page.bounds(&format!("row:{dir}")).is_some()
        })?;
        page.click(&format!("row:{dir}"))?;
    }
    wait(&mut page, |page| {
        [
            "row:out/deep/inner.txt",
            "row:logs/today.log",
            "row:build/source.txt",
        ]
        .iter()
        .all(|row| page.bounds(row).is_some())
    })?;
    let quiet = ink_of(&page, "out");
    let plain = ink_of(&page, "kept.txt");
    assert_ne!(quiet, plain);
    assert_eq!(ink_of(&page, "deep"), quiet);
    assert_eq!(ink_of(&page, "inner.txt"), quiet);
    assert_eq!(ink_of(&page, "today.log"), quiet);
    assert_eq!(
        ink_of(&page, "logs"),
        plain,
        "only what is in it is ignored"
    );
    assert_eq!(ink_of(&page, "build"), quiet, "quiet by its name");
    assert_eq!(ink_of(&page, "source.txt"), plain, "git keeps it");
    Ok(())
}

/// The setting leaves out of the tree what git ignores and nothing else: not a folder quiet by its name,
/// not what is beside an ignored file. What was chosen goes to the folder it is in, and all comes back.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_setting_leaves_what_git_ignores_out_of_the_tree() -> Result<(), Error> {
    let project = lantern();
    project.write(".gitignore", "out/\n*.log\n");
    project.write("out/deep/inner.txt", "x\n");
    project.write("logs/today.log", "x\n");
    project.write("logs/kept.txt", "x\n");
    project.write("build/source.txt", "x\n");
    crate::page_changes::committed(&project);
    let session = project.session(&[]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+shift+e")?;
    for dir in ["out", "logs", "build"] {
        wait(&mut page, |page| {
            page.bounds(&format!("row:{dir}")).is_some()
        })?;
        page.click(&format!("row:{dir}"))?;
    }
    let gone = ["row:out", "row:out/deep", "row:logs/today.log"];
    let kept = [
        "row:.gitignore",
        "row:logs",
        "row:logs/kept.txt",
        "row:build",
        "row:build/source.txt",
    ];
    wait(&mut page, |page| {
        gone.iter()
            .chain(&kept)
            .all(|row| page.bounds(row).is_some())
    })?;
    page.click("row:logs/today.log")?;
    page.press("f1")?;
    page.type_text("Toggle Ignored Files in the Tree")?;
    page.advance(0.3)?;
    page.press("enter")?;
    wait(&mut page, |page| {
        gone.iter().all(|row| page.bounds(row).is_none())
    })?;
    for row in kept {
        assert!(page.bounds(row).is_some(), "{row} is not ignored");
    }
    wait(&mut page, |_| session.settings().hide_ignored)?;
    page.press("f2")?;
    assert_eq!(page.field("dialog-input"), Some("logs"), "what it is in");
    page.press("escape")?;
    page.press("f1")?;
    page.type_text("Toggle Ignored Files in the Tree")?;
    page.advance(0.3)?;
    page.press("enter")?;
    wait(&mut page, |page| {
        gone.iter().all(|row| page.bounds(row).is_some())
    })?;
    wait(&mut page, |_| !session.settings().hide_ignored)?;
    Ok(())
}

/// In a pane too short for all the settings, the keys bring the one they come to into view.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_setting_the_keys_come_to_is_in_view() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[]);
    let mut page = project.open(&session, PANE)?;
    page.press("cmd+,")?;
    for _ in 0..8 {
        page.press("down")?;
    }
    page.advance(0.5)?;
    page.settle()?;
    let body = page.bounds("settings-body").expect("the settings");
    let last = page.bounds("setting:9").expect("the last setting");
    assert!(
        last.y + last.h <= body.y + body.h + 0.5,
        "{last:?} is below {body:?}"
    );
    for _ in 0..8 {
        page.press("up")?;
    }
    page.advance(0.5)?;
    page.settle()?;
    let first = page.bounds("setting:1").expect("the first setting");
    assert!(first.y >= body.y - 0.5, "{first:?} is above {body:?}");
    Ok(())
}

/// ⌫ in a palette with nothing typed goes back to the files, from the commands and from a line.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn backspace_in_an_empty_palette_goes_back_to_the_files() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let says = |page: &Page, mode: &str| page_words(page).iter().any(|word| word == mode);
    page.press("cmd+k")?;
    page.type_text(">")?;
    assert!(says(&page, "Commands"), "{:?}", page_words(&page));
    page.press("backspace")?;
    assert!(says(&page, "Files"), "{:?}", page_words(&page));
    assert!(keyboard_is_in(&page, "palette-query"), "{:?}", page.focus());
    page.type_text(":")?;
    assert!(says(&page, "Line"), "{:?}", page_words(&page));
    page.press("backspace")?;
    assert!(says(&page, "Files"), "{:?}", page_words(&page));
    // In the files themselves it is the field's: it takes back what was typed.
    page.type_text("ab")?;
    page.press("backspace")?;
    assert_eq!(page.field("palette-query"), Some("a"));
    assert!(says(&page, "Files"), "{:?}", page_words(&page));
    Ok(())
}

/// The palette's rows stay 30 points however long the list is (the list is as high as whole rows),
/// the files carry no "open" word for the tabs that are, the foot writes its keys as caps (Enter is a
/// word) and a short pane has no foot.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_palette_keeps_its_rows_whole_and_says_its_keys_as_words() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts", "src/lib/orders.ts"]);
    let mut page = project.open(&session, WIDE)?;
    page.press("cmd+k")?;
    let words = page_words(&page);
    assert!(!words.iter().any(|word| word == "open"), "{words:?}");
    assert!(
        words.iter().any(|word| word == "Enter") && !words.iter().any(|word| word.contains('⏎')),
        "{words:?}"
    );
    let first = page.bounds("palette-item:1").expect("a row");
    let second = page.bounds("palette-item:2").expect("a second row");
    assert_eq!((first.h, second.h), (30.0, 30.0));
    // A long list (every command) is cut between rows, not through one.
    page.type_text(">")?;
    let list = page.bounds("palette-list").expect("the list");
    assert_eq!(list.h % 30.0, 0.0, "{list:?}");
    assert_eq!(page.bounds("palette-item:9").expect("a row").h, 30.0);
    page.press("escape")?;
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, PANE)?;
    page.press("cmd+k")?;
    let words = page_words(&page);
    assert!(
        shows(&page, "server.ts") && !words.iter().any(|word| word == "Enter"),
        "{words:?}"
    );
    Ok(())
}

/// A command with several keys is done by each of them; a menu's entries show their keys as a
/// command's are written.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_command_is_done_by_each_of_its_keys_and_a_menu_shows_its_keys() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["src/server.ts", "README.md", "config.yaml"]);
    let mut page = project.open(&session, WIDE)?;
    let in_front = |page: &Page| page_words(page).iter().any(|word| word == "config.yaml");
    page.press("ctrl+tab")?;
    wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
    page.press("ctrl+page_down")?;
    wait(&mut page, |page| text(page).starts_with("# ports"))?;
    assert!(in_front(&page));
    page.press("ctrl+shift+tab")?;
    wait(&mut page, |page| text(page).starts_with("# Lantern"))?;
    page.press("ctrl+page_up")?;
    wait(&mut page, |page| text(page) == SERVER)?;
    // Delete Line, by either of its keys.
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.press("ctrl+shift+k")?;
    page.advance(0.5)?;
    assert!(
        text(&page).starts_with("\nimport { formatPrice }"),
        "{:?}",
        text(&page)
    );
    page.press("cmd+shift+k")?;
    page.advance(0.5)?;
    assert!(
        text(&page).starts_with("import { formatPrice }"),
        "{:?}",
        text(&page)
    );
    // A tab's menu: Close with its keys as a person writes them.
    page.right_click("tab:README.md")?;
    let menu = page.menu().expect("a menu");
    let keys_of = |title: &str| {
        menu.entries.iter().find_map(|entry| match entry {
            MenuEntry::Item {
                title: named, keys, ..
            } if named == title => Some(*keys),
            _ => None,
        })
    };
    assert_eq!(keys_of("Close"), Some(Shortcut::parse("ctrl+w")));
    assert_eq!(keys_of("Rename"), Some(Shortcut::parse("f2")));
    assert_eq!(keys_of("Copy Path"), Some(None));
    Ok(())
}

/// The commands of whole lines are the editor's own.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_line_commands_are_the_editors() -> Result<(), Error> {
    let project = lantern();
    project.write("lines.txt", "one\ntwo\nthree\nfour\n");
    let session = project.session(&["lines.txt"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let selected = |page: &Page| {
        let buffer = page.editor(&editor)?;
        let range = buffer.ranges().first().copied()?;
        let (from, to) = (range.anchor.min(range.head), range.anchor.max(range.head));
        Some(
            buffer
                .text()
                .chars()
                .skip(from)
                .take(to - from)
                .collect::<String>(),
        )
    };
    page.caret_to(&editor, 5)?;
    page.press("cmd+i")?;
    assert_eq!(selected(&page).as_deref(), Some("two\n"));
    page.press("ctrl+j")?;
    page.advance(0.5)?;
    assert_eq!(text(&page), "one\ntwo three\nfour\n");
    page.caret_to(&editor, 1)?;
    page.command("insert-line")?;
    page.advance(0.5)?;
    assert_eq!(text(&page), "one\n\ntwo three\nfour\n");
    page.command("duplicate-line-up")?;
    page.advance(0.5)?;
    assert_eq!(text(&page), "one\n\n\ntwo three\nfour\n");
    // Fold All folds what folds: a file of two blocks has two regions folded.
    project.write(
        "blocks.py",
        "def a():\n    x = 1\n    y = 2\n\ndef b():\n    z = 3\n    w = 4\n",
    );
    go_to(&mut page, "blocks.py")?;
    let editor = shown(&page).expect("an editor");
    page.press("ctrl+alt+[")?;
    page.advance(0.5)?;
    assert_eq!(
        page.editor(&editor).map(|buffer| buffer.folded().len()),
        Some(2)
    );
    Ok(())
}

/// A file that changed on disk, with nothing of ours in it, is changed where it differs: what the
/// person did before can still be undone, and the text that came from the disk stays.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_clean_file_read_again_keeps_what_can_be_undone() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    page.caret_to(&editor, 0)?;
    page.type_text("A ")?;
    page.advance(1.0)?;
    page.press("cmd+s")?;
    wait(&mut page, |_| {
        project.read("README.md").starts_with("A # Lantern")
    })?;
    page.advance(1.0)?;
    // Another program adds to the end and to the middle of the file.
    let on_disk = format!("{}\nAdded by another program.\n", project.read("README.md"))
        .replace("API", "service");
    project.write("README.md", &on_disk);
    wait(&mut page, |page| text(page) == on_disk)?;
    page.advance(0.5)?;
    assert_eq!(shown(&page), Some(editor), "the same editor");
    // The typing is undone, and the disk's words are still there.
    page.press("cmd+z")?;
    page.advance(0.5)?;
    assert_eq!(text(&page), on_disk.replacen("A # Lantern", "# Lantern", 1));
    // The change from the disk is nobody's to be sent back or saved: nothing is unsaved.
    page.advance(1.0)?;
    assert!(page.bounds("tab:README.md").is_some());
    Ok(())
}

/// The strip of tabs is the person's to scroll: it is brought to the tab in front when another tab
/// comes to the front, not after every turn of the wheel.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_tabs_scroll_away_from_the_tab_in_front_and_come_to_the_next_one() -> Result<(), Error> {
    let project = lantern();
    let session = project.session(&[
        "README.md",
        "src/server.ts",
        "src/lib/format.ts",
        "src/lib/orders.ts",
        "src/routes/refunds.ts",
        "worker/mailer.py",
        "config.yaml",
        "Dockerfile",
        "notes/emoji.txt",
    ]);
    let mut page = project.open(&session, PANE)?;
    let strip = page.bounds("tabs").expect("the strip of tabs");
    let front = page.bounds("tab:README.md").expect("the tab in front");
    assert!(
        front.x >= strip.x,
        "in view at first: {front:?} in {strip:?}"
    );
    // The wheel takes the strip to its other end, and it stays there.
    page.wheel("tabs", 2000.0, 0.0)?;
    page.advance(1.0)?;
    let front = page.bounds("tab:README.md").expect("the tab in front");
    assert!(
        front.x + front.w <= strip.x,
        "the strip stays where the wheel took it: {front:?} in {strip:?}"
    );
    // Another tab to the front is brought into view.
    go_to(&mut page, "src/server.ts")?;
    let next = page.bounds("tab:src/server.ts").expect("the tab in front");
    assert!(
        next.x >= strip.x && next.x + next.w <= strip.x + strip.w,
        "the tab in front is in view: {next:?} in {strip:?}"
    );
    Ok(())
}

/// Every open file's editor takes its text as the terminal makes it (`editor_ready`), shown or
/// hidden, with nothing refused.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn every_editor_takes_its_text_when_it_is_made() -> Result<(), Error> {
    let project = lantern();
    let tabs = [
        "src/server.ts",
        "README.md",
        "config.yaml",
        "Dockerfile",
        "worker/mailer.py",
    ];
    let session = project.session(&tabs);
    let mut page = project.open(&session, PANE)?;
    for (n, tab) in tabs.iter().enumerate().skip(1) {
        page.press(&format!("ctrl+{}", n + 1))?;
        let expected = project.read(tab);
        wait(&mut page, |page| text(page) == expected)?;
    }
    assert!(!shows(&page, "did not do it"), "{:?}", page_words(&page));
    Ok(())
}

/// A place opened at puts its line in the middle of the editor, and the other places of what is
/// selected are marked (the editor's `matches`).
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn a_place_is_centred_and_a_selection_marks_its_other_places() -> Result<(), Error> {
    let project = lantern();
    let long: String = (1..=400).map(|n| format!("line {n}\n")).collect();
    project.write("long.txt", &long);
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    // Where the words of a line are drawn, down the page (the editor's lines are text of the draw list).
    let y_of = |page: &Page, words: &str| {
        page.list().items.iter().find_map(|item| match item {
            Paint::Text(run) if run.text == words => Some(run.origin.y),
            _ => None,
        })
    };
    go_to(&mut page, "long.txt:150")?;
    let editor = shown(&page).expect("an editor");
    let room = page.bounds(&editor).expect("its room");
    let (at, next) = (y_of(&page, "line 150"), y_of(&page, "line 151"));
    let (at, row) = (
        at.expect("the line"),
        next.expect("the next") - at.expect("the line"),
    );
    assert!(row > 8.0, "a row of {row}");
    // The line is in the middle of the room, not at its edge as a place shown by scrolling as little
    // as it takes would leave it.
    let middle = room.y + room.h / 2.0;
    assert!(
        (at - middle).abs() <= row * 1.5,
        "{at} is not about {middle} (a row is {row})"
    );
    let marks = format!(
        "{:?}",
        page.editor(&editor).map(|buffer| buffer.options().matches)
    );
    assert!(marks.contains("Selection"), "{marks}");
    Ok(())
}

/// The repository going away is `git: null` in the page's changes, not the same as nothing said of
/// it: the branch leaves the status line.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_branch_leaves_the_status_line_when_the_repository_goes() -> Result<(), Error> {
    let project = lantern();
    let root = project.root();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .env("GIT_AUTHOR_NAME", "textpen")
            .env("GIT_AUTHOR_EMAIL", "textpen@example.com")
            .env("GIT_COMMITTER_NAME", "textpen")
            .env("GIT_COMMITTER_EMAIL", "textpen@example.com")
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "lantern-branch"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "The shop"]);
    let session = project.session(&["README.md"]);
    let mut page = project.open(&session, WIDE)?;
    let branch = |page: &Page| page_words(page).iter().any(|word| word == "lantern-branch");
    wait(&mut page, &branch)?;
    std::fs::remove_dir_all(root.join(".git")).expect("the repository");
    wait(&mut page, |page| !branch(page))?;
    Ok(())
}

// --- The editor's menu ------------------------------------------------------------------------------

/// The first line of `SERVER`, and where its other lines are: the menu is pressed at the end of one.
const IMPORT_LINE: &str = "import { createServer } from \"node:http\";";

/// The person presses the right button at the end of line `line` of the editor `name`, the line's
/// row found by the column of changes (a project that is a repository has it).
fn right_press_at_line(page: &mut Page, name: &str, line: usize) -> Result<(), Error> {
    use pencere_view::{Button, Input, Mods};
    // A menu still open is closed by the person's next press, which is not the page's.
    if page.menu().is_some() {
        page.press("escape")?;
    }
    let room = page
        .bounds(name)
        .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
    let cell = page
        .gutter_cell(name, "changes", line)
        .ok_or_else(|| Error::NoSuch(format!("line {line}")))?;
    let at = Point::new(room.x + room.w - 60.0, cell.y + cell.h / 2.0);
    page.pointer(Input::Press {
        at,
        button: Button::Right,
        mods: Mods::default(),
        count: 1,
    })?;
    page.pointer(Input::Release {
        at,
        button: Button::Right,
    })
}

/// The titles of the menu open, a rule as `-`.
fn menu_titles(page: &Page) -> Vec<String> {
    page.menu()
        .expect("a menu")
        .entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item { title, .. } | MenuEntry::Paste { title, .. } => title.clone(),
            MenuEntry::Separator => "-".to_owned(),
        })
        .collect()
}

/// A right press outside the selection puts the caret there and opens the editor's menu: the
/// library's lines with their keys and textpen's own after them; inside the selection it stays.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_editors_menu_has_the_standard_lines_and_textpens_own_and_the_press_puts_the_caret()
-> Result<(), Error> {
    let project = lantern();
    crate::page_changes::committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let ranges = |page: &Page| {
        page.editor(&editor)
            .map(|buffer| {
                buffer
                    .ranges()
                    .iter()
                    .map(|range| (range.anchor, range.head))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    // Outside the selection: one caret at the press, and the menu.
    page.select(&editor, 0, 6)?;
    right_press_at_line(&mut page, &editor, 2)?;
    let end_of_line_2 = IMPORT_LINE.chars().count()
        + 2
        + "import { formatPrice } from \"./lib/format.ts\";"
            .chars()
            .count();
    assert_eq!(ranges(&page), vec![(end_of_line_2, end_of_line_2)]);
    assert_eq!(
        menu_titles(&page),
        [
            "Cut",
            "Copy",
            "Paste",
            "-",
            "Select All",
            "-",
            "Toggle Line Comment",
            "Select All Occurrences",
            "-",
            "Find in Project",
            "Compare with Last Commit",
            "Command Palette"
        ]
    );
    let menu = page.menu().expect("a menu");
    let keys_of = |title: &str| {
        menu.entries.iter().find_map(|entry| match entry {
            MenuEntry::Item {
                title: named, keys, ..
            } if named == title => Some(*keys),
            _ => None,
        })
    };
    assert_eq!(keys_of("Copy"), Some(Shortcut::parse("cmd+c")));
    assert_eq!(keys_of("Select All"), Some(Shortcut::parse("cmd+a")));
    assert_eq!(
        keys_of("Find in Project"),
        Some(Shortcut::parse("cmd+shift+f"))
    );
    assert_eq!(keys_of("Command Palette"), Some(Shortcut::parse("f1")));
    // No change on that line: no Revert Change.
    assert!(!menu_titles(&page).contains(&"Revert Change".to_owned()));
    // Inside the selection (its end counts): the selection stays.
    let line_end = IMPORT_LINE.chars().count();
    page.select(&editor, 0, line_end)?;
    right_press_at_line(&mut page, &editor, 0)?;
    assert_eq!(ranges(&page), vec![(0, line_end)]);
    assert!(page.menu().is_some());
    Ok(())
}

/// Copy puts the selection on the pasteboard, and Paste (the host's own line) puts the pasteboard in
/// where the press put the caret.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_menus_copy_and_paste_go_by_the_pasteboard() -> Result<(), Error> {
    let project = lantern();
    crate::page_changes::committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let line_end = IMPORT_LINE.chars().count();
    page.select(&editor, 0, line_end)?;
    right_press_at_line(&mut page, &editor, 0)?;
    page.pick_titled("Copy")?;
    assert_eq!(page.pasteboard(), Some(IMPORT_LINE));
    right_press_at_line(&mut page, &editor, 4)?;
    page.pick_titled("Paste")?;
    wait(&mut page, |page| {
        text(page).contains(&format!("?? 8080);{IMPORT_LINE}"))
    })?;
    Ok(())
}

/// Revert Change is a line of the menu only where the line is in a change git has, and takes it back.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_menus_revert_change_is_on_a_changed_line_only() -> Result<(), Error> {
    let project = lantern();
    crate::page_changes::committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let place = SERVER.find("8080").expect("the port") + 4;
    page.caret_to(&editor, place)?;
    page.type_text("1")?;
    wait(&mut page, |page| {
        !crate::page_changes::marks(page).is_empty()
    })?;
    right_press_at_line(&mut page, &editor, 2)?;
    assert!(!menu_titles(&page).contains(&"Revert Change".to_owned()));
    right_press_at_line(&mut page, &editor, 4)?;
    let titles = menu_titles(&page);
    assert!(titles.contains(&"Revert Change".to_owned()), "{titles:?}");
    assert_eq!(
        titles[titles.len() - 4..],
        [
            "Find in Project",
            "Revert Change",
            "Compare with Last Commit",
            "Command Palette"
        ]
    );
    page.pick_titled("Revert Change")?;
    wait(&mut page, |page| text(page) == SERVER)?;
    Ok(())
}

/// Find in Project seeds the search with the word at the caret; Command Palette opens the palette
/// of commands; Shift+F10 opens the menu at the caret, and the command is listed.
#[test]
#[ignore = "page: compiles the kit's interpreter and drives the page; make test-page"]
fn the_menus_own_lines_search_and_list_commands_and_the_keyboard_opens_it() -> Result<(), Error> {
    let project = lantern();
    crate::page_changes::committed(&project);
    let session = project.session(&["src/server.ts"]);
    let mut page = project.open(&session, WIDE)?;
    let editor = shown(&page).expect("an editor");
    let in_port = SERVER.find("PORT").expect("a word") + 2;
    page.caret_to(&editor, in_port)?;
    page.press("shift+f10")?;
    let menu = page.menu().expect("a menu at the caret");
    assert!(menu.at.x.is_finite() && menu.at.y > 0.0, "{:?}", menu.at);
    assert!(menu_titles(&page).contains(&"Cut".to_owned()));
    page.pick_titled("Find in Project")?;
    wait(&mut page, |page| shows(page, "const PORT = Number"))?;
    assert_eq!(page.field("search-query"), Some("PORT"));
    page.caret_to(&editor, in_port)?;
    page.press("shift+f10")?;
    page.pick_titled("Command Palette")?;
    page.type_text("editor menu")?;
    wait(&mut page, |page| shows(page, "Show Editor Menu"))?;
    Ok(())
}
