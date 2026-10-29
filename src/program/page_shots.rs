//! The pictures of docs/, made from the page as it is: the page and the program's handlers in one
//! process (as `page_tests` runs them), laid out and drawn in the fonts of this machine (macOS: SF
//! Mono, or Menlo, for the terminal's face, as bateri's default is; SF for the system's) by
//! pencere's host with no screen. Not a terminal's drawing — no shaping, no shadows, and not the
//! host's text fields over the page, so a field is drawn empty — but the same boxes, words, colours
//! and editors where the kit puts them.
//!
//! They need the system's fonts, so they are not part of the gate: `make shots` makes them.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use pencere_drive::headless::{Error, InProcess};
use pencere_drive::shot::{Fonts, Palette};

use crate::options;
use crate::page_tests::{PANE, Page, Project, go_to, lantern, open_with, shown, shows, wait};

/// The fonts a Mac draws a terminal and its system in: bateri's terminal face is SF Mono where the
/// system has it, else Menlo.
fn fonts() -> Fonts {
    let read = |path: &str| {
        std::fs::read(path)
            .unwrap_or_else(|why| panic!("{path}: {why} (make shots needs the system's fonts)"))
    };
    let terminal = std::fs::read("/System/Library/Fonts/SFNSMono.ttf")
        .unwrap_or_else(|_| read("/System/Library/Fonts/Menlo.ttc"));
    Fonts::none()
        .terminal(&terminal)
        .and_then(|fonts| fonts.system(&read("/System/Library/Fonts/SFNS.ttf")))
        .expect("the fonts")
}

/// The project made a git repository, with a change or two since its last commit: what the tree
/// and the status line say of git.
fn with_git(project: &Project) {
    let root = project.root();
    let git = |args: &[&str]| {
        let status = Command::new("git")
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
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "The shop"]);
    project.write(
        "README.md",
        &format!("{}\n- [ ] receipts by mail\n", project.read("README.md")),
    );
    project.write(
        "src/lib/format.ts",
        &format!(
            "{}export const SHIPPING = 499;\n",
            project.read("src/lib/format.ts")
        ),
    );
    // The file the editor's picture shows: a line changed, two added and two taken away, as a
    // person working in it would have left it.
    let server = project
        .read("src/server.ts")
        .replace("8080", "3000")
        .replace(
            "/** Answers with JSON. */\n",
            "/** Answers with JSON. */\n// Every price is in cents.\n// Nothing here rounds.\n",
        )
        .replace("import { formatPrice } from \"./lib/format.ts\";\n\n", "");
    project.write("src/server.ts", &server);
    project.write(
        "src/routes/products.ts",
        "export const products = [\n  { name: \"Brass Lantern\", price: 1999 },\n];\n",
    );
}

fn docs(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join(name)
}

fn open(project: &Project, tabs: &[&str], size: (f32, f32), fonts: &Fonts) -> Result<Page, Error> {
    let session = project.session(tabs);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_with(program, size, Some(fonts))?;
    wait(&mut page, |page| shown(page).is_some() || tabs.is_empty())?;
    // The language is read and the git said, a few frames on.
    page.advance(1.5)?;
    Ok(page)
}

fn save(page: &Page, fonts: &Fonts, palette: Palette, name: &str) {
    let png = page.shot_in(fonts, palette).expect("a picture");
    std::fs::write(docs(name), png).expect("the picture written");
}

#[test]
#[ignore = "shots: draws the page in the fonts of this machine into docs/; make shots"]
fn the_pictures_of_the_docs() -> Result<(), Error> {
    let fonts = fonts();

    // The editor: a TypeScript file among its tabs, the tree open on its folders.
    let project = lantern();
    with_git(&project);
    let mut page = open(
        &project,
        &[
            "src/server.ts",
            "src/lib/format.ts",
            "README.md",
            "worker/mailer.py",
        ],
        (1280.0, 800.0),
        &fonts,
    )?;
    page.press("cmd+shift+e")?;
    for folder in ["row:src/lib", "row:src/routes"] {
        page.click(folder)?;
        page.advance(0.3)?;
    }
    page.press("escape")?;
    go_to(&mut page, "src/server.ts:19:5")?;
    page.advance(1.5)?;
    save(&page, &fonts, Palette::Dark, "editor.png");

    // A change peeked: the lines as they were on red, as they are on green, in a gap of the editor.
    page.press("alt+f5")?;
    page.advance(0.5)?;
    save(&page, &fonts, Palette::Dark, "changes.png");
    page.press("escape")?;

    // The palette: the project's files, those seen last first, then those that go with "ord" (what
    // is typed into its field, and its placeholder, are the host's to draw, and are not in the
    // pictures).
    go_to(&mut page, "src/lib/format.ts")?;
    page.press("cmd+k")?;
    page.advance(0.5)?;
    save(&page, &fonts, Palette::Dark, "palette-empty.png");
    page.type_text("ord")?;
    page.advance(0.5)?;
    save(&page, &fonts, Palette::Dark, "palette.png");
    page.press("escape")?;

    // The search in the project's files, a match open.
    page.press("cmd+shift+f")?;
    page.type_text("order")?;
    page.advance(0.5)?;
    wait(&mut page, |page| shows(page, "results in"))?;
    page.press("down")?;
    page.press("down")?;
    page.press("down")?;
    page.press("space")?;
    page.advance(1.5)?;
    save(&page, &fonts, Palette::Dark, "search.png");

    // A Markdown file beside its preview, in a light terminal.
    let project = lantern();
    with_git(&project);
    let mut page = open(&project, &["README.md"], (1100.0, 680.0), &fonts)?;
    page.press("cmd+shift+v")?;
    page.advance(1.0)?;
    save(&page, &fonts, Palette::Light, "preview.png");

    // An 80×24 pane, in a light terminal: the editor alone, the tree in its drawer.
    let project = lantern();
    with_git(&project);
    let page = open(
        &project,
        &["src/lib/format.ts", "src/server.ts"],
        PANE,
        &fonts,
    )?;
    save(&page, &fonts, Palette::Light, "light-80x24.png");
    Ok(())
}
