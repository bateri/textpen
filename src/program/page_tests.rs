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
use crate::state::defaults::UiState;
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
/// call, 2 s to start and 20 s to compile, which are the wall clock's, are far wider here by
/// default (`GENEROUS_CALL`). With `TEXTPEN_CALL_MS` set (`make check-timing`) a call is held to that
/// many milliseconds, the rest to the kit's own: the page in the time a terminal gives it, or in
/// half of it (`TEXTPEN_CALL_MS=125`), to see how near it comes. Meant for a machine at rest.
pub(crate) fn limits(call_ms: Option<u64>) -> pencere_sandbox::ModuleLimits {
    let kit = pencere_sandbox::ModuleLimits::default();
    match call_ms {
        Some(ms) => pencere_sandbox::ModuleLimits {
            call: Duration::from_millis(ms),
            ..kit
        },
        None => pencere_sandbox::ModuleLimits {
            call: GENEROUS_CALL,
            start: GENEROUS_CALL * 6,
            compile: GENEROUS_CALL * 12,
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
    let mut page = opening.peer(program).open()?;
    // What the page reads first comes from the program on its own threads: the file in front, or
    // with none the start page.
    page.wait_for(WITHIN, |page| {
        shown(page).is_some() || page_words(page).iter().any(|word| word == "Go to File")
    })?;
    page.advance(0.5)?;
    Ok(page)
}

/// The name of the editor shown (an editor is named by its tab: `doc:3`).
pub(crate) fn shown(page: &Page) -> Option<String> {
    let laid = page.laid()?;
    let room = laid.editors().into_iter().find(|room| !room.hidden)?;
    (1..500)
        .map(|n| format!("doc:{n}"))
        .find(|name| Id::named(name) == room.id)
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
    page.pick_titled("Delete…")?;
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
    assert_eq!(keys_of("Rename…"), Some(Shortcut::parse("f2")));
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
