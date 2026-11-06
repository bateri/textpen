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
use crate::page_compare::{drawn, left_of};
use crate::page_pdf::{Run, pages_drawn, pdf_of_pages, pdf_of_runs};
use crate::page_tests::{
    PANE, Page, Project, go_to, lantern, open_holding, open_with, pictures_drawn, png, recording,
    shown, shows, wait,
};
use crate::page_uploads::{
    give, handle, moving, open_quick_in, over_row, pass, through, wait as wait_through,
};

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

/// A picture to show: a lantern's glow on a dark ground, 1600 × 1000 pixels, larger than the room the tab has.
fn lantern_art() -> Vec<u8> {
    let (width, height) = (1600_u32, 1000_u32);
    png(width, height, |x, y| {
        let (u, v) = (
            f64::from(x) / f64::from(width),
            f64::from(y) / f64::from(height),
        );
        let ground = [14.0 + 14.0 * v, 24.0 + 24.0 * v, 40.0 + 40.0 * v];
        let (dx, dy) = ((u - 0.5) * 1.6, v - 0.46);
        let glow = (1.0 - (dx * dx + dy * dy).sqrt() / 0.42)
            .clamp(0.0, 1.0)
            .powf(1.6);
        let body = (u - 0.5).abs() < 0.07 && (v - 0.5).abs() < 0.17;
        let warm = [255.0, 196.0, 92.0];
        let mut pixel = [0_u8; 3];
        for (n, out) in pixel.iter_mut().enumerate() {
            let lit = ground[n] * (1.0 - glow) + warm[n] * glow;
            let value = if body { lit * 0.55 + 30.0 } else { lit };
            // The channel is in 0..=255 by the clamp.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let channel = value.clamp(0.0, 255.0) as u8;
            *out = channel;
        }
        pixel
    })
}

/// As [`open`], on a terminal that holds the pictures `files` already.
fn open_pictured(
    project: &Project,
    tabs: &[&str],
    size: (f32, f32),
    fonts: &Fonts,
    files: &[&[u8]],
) -> Result<Page, Error> {
    let session = project.session(tabs);
    recording(&session);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_holding(program, size, Some(fonts), 0.0, files)?;
    wait(&mut page, |page| {
        shown(page).is_some() || !pictures_drawn(page).is_empty() || tabs.is_empty()
    })?;
    // The picture is decoded and drawn, a few frames on.
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
    let art = lantern_art();
    project.write_bytes("docs/lantern.png", &art);
    project.write(
        "README.md",
        &project.read("README.md").replacen(
            "mail.\n",
            "mail.\n\n![A brass lantern, lit](docs/lantern.png)\n",
            1,
        ),
    );
    with_git(&project);
    let mut page = open_pictured(&project, &["README.md"], (1100.0, 680.0), &fonts, &[&art])?;
    page.press("cmd+shift+v")?;
    wait(&mut page, |page| !pictures_drawn(page).is_empty())?;
    page.advance(1.0)?;
    save(&page, &fonts, Palette::Light, "preview.png");

    // A picture in its tab, fitted, with its size under it.
    let project = lantern();
    let art = lantern_art();
    project.write_bytes("assets/lantern.png", &art);
    with_git(&project);
    let mut page = open_pictured(
        &project,
        &["assets/lantern.png", "src/server.ts", "README.md"],
        (1280.0, 800.0),
        &fonts,
        &[&art],
    )?;
    page.advance(0.5)?;
    save(&page, &fonts, Palette::Dark, "picture.png");

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

/// The project made a repository, and the file the comparison is shown on changed as a person
/// working in it would have left it: a line changed, a line made into three, two added at the top and
/// two taken away.
fn with_changed_server(project: &Project) {
    with_git(project);
    let server = project.read("src/server.ts").replace(
        "  send(res, 404, { error: `no route for ${req.method}` });\n",
        "  // Nothing else is served.\n  const known = false;\n  send(res, 404, { error: `no route for ${req.method}`, known });\n",
    );
    project.write("src/server.ts", &server);
}

/// The pictures of the comparison for the docs: the file beside the last commit, and the panel of the
/// changed files with a file shown.
#[test]
#[ignore = "shots: draws the page in the fonts of this machine into docs/; make shots"]
fn the_pictures_of_the_comparison_in_the_docs() -> Result<(), Error> {
    let fonts = fonts();
    let project = lantern();
    with_changed_server(&project);
    let mut page = open(&project, &["src/server.ts"], (1280.0, 800.0), &fonts)?;
    page.press("cmd+shift+d")?;
    wait(&mut page, drawn)?;
    page.advance(1.0)?;
    page.settle()?;
    save(&page, &fonts, Palette::Dark, "compare.png");

    let project = lantern();
    with_changed_server(&project);
    std::fs::remove_file(project.root().join("src/lib/orders.ts")).expect("removed");
    project.write("notes/todo.txt", "ship the lantern\n");
    let mut page = open(&project, &["README.md"], (1280.0, 800.0), &fonts)?;
    page.press("cmd+shift+g")?;
    wait(&mut page, |page| shows(page, "Unversioned Files"))?;
    page.click("change:src/server.ts")?;
    wait(&mut page, drawn)?;
    page.advance(1.0)?;
    page.settle()?;
    save(&page, &fonts, Palette::Dark, "changes-panel.png");
    Ok(())
}

/// The file shown beside what the last commit keeps of it, in a dark and a light terminal, wide and
/// in an 80×24 pane, for the eye (`target/shots/`).
#[test]
#[ignore = "shots: draws the page in the fonts of this machine into target/shots/"]
fn the_pictures_of_the_comparison() -> Result<(), Error> {
    let fonts = fonts();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shots");
    std::fs::create_dir_all(&out).expect("the folder");
    for (name, size, palette) in [
        ("compare-dark-wide", (1280.0, 800.0), Palette::Dark),
        ("compare-light-wide", (1280.0, 800.0), Palette::Light),
        ("compare-dark-80x24", PANE, Palette::Dark),
        ("compare-light-80x24", PANE, Palette::Light),
    ] {
        let project = lantern();
        with_changed_server(&project);
        let mut page = open(&project, &["src/server.ts"], size, &fonts)?;
        page.press("cmd+shift+d")?;
        wait(&mut page, drawn)?;
        page.advance(1.0)?;
        page.settle()?;
        assert!(left_of(&page).is_some());
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    // Long stretches the same, folded on both sides, and the chevrons that take a change back.
    for (name, size, palette) in [
        ("fold-dark-wide", (1280.0, 800.0), Palette::Dark),
        ("fold-light-80x24", PANE, Palette::Light),
    ] {
        let project = lantern();
        let old: String = (0..80).map(|n| format!("const v{n} = {n};\n")).collect();
        project.write("src/long.ts", &old);
        with_git(&project);
        project.write(
            "src/long.ts",
            &old.replace("const v10 = 10;\n", "const v10 = 1000;\n")
                .replace("const v40 = 40;\nconst v41 = 41;\n", "")
                .replace(
                    "const v70 = 70;\n",
                    "const v70 = 70;\n// added a\n// added b\n",
                ),
        );
        let mut page = open(&project, &["src/long.ts"], size, &fonts)?;
        page.press("cmd+shift+d")?;
        wait(&mut page, drawn)?;
        page.advance(1.0)?;
        page.settle()?;
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    // The panel of changes, with a file shown beside the last commit: wide, and in an 80×24 pane where
    // it is a drawer over the editor.
    for (name, size, palette) in [
        ("panel-dark-wide", (1280.0, 800.0), Palette::Dark),
        ("panel-light-wide", (1280.0, 800.0), Palette::Light),
        ("panel-dark-80x24", PANE, Palette::Dark),
        ("panel-light-80x24", PANE, Palette::Light),
    ] {
        let project = lantern();
        with_changed_server(&project);
        std::fs::remove_file(project.root().join("src/lib/orders.ts")).expect("removed");
        project.write("notes/todo.txt", "ship the lantern\n");
        let mut page = open(&project, &["README.md"], size, &fonts)?;
        page.press("cmd+shift+g")?;
        wait(&mut page, |page| shows(page, "Unversioned Files"))?;
        if size.0 > 700.0 {
            page.click("change:src/server.ts")?;
            wait(&mut page, drawn)?;
        }
        page.advance(1.0)?;
        page.settle()?;
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    // The same panel with its files under their folders.
    for (name, size, palette) in [
        ("panel-tree-dark-wide", (1280.0, 800.0), Palette::Dark),
        ("panel-tree-light-wide", (1280.0, 800.0), Palette::Light),
        ("panel-tree-dark-80x24", PANE, Palette::Dark),
    ] {
        let project = lantern();
        with_changed_server(&project);
        std::fs::remove_file(project.root().join("src/lib/orders.ts")).expect("removed");
        project.write("notes/todo.txt", "ship the lantern\n");
        let mut page = open(&project, &["README.md"], size, &fonts)?;
        page.press("cmd+shift+g")?;
        wait(&mut page, |page| shows(page, "Unversioned Files"))?;
        page.press("t")?;
        page.advance(1.0)?;
        page.settle()?;
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    // The sidebar as narrow as its edge goes, at a terminal's larger text: the buttons of its head and
    // the search's field.
    for (name, keys) in [
        ("sidebar-narrow-files", "cmd+shift+e"),
        ("sidebar-narrow-search", "cmd+shift+f"),
    ] {
        let project = lantern();
        let mut page = open(&project, &["README.md"], (1280.0, 800.0), &fonts)?;
        page.zoom(1.12)?;
        page.settle()?;
        let edge = page.bounds("resizer").expect("the edge");
        let at = edge.y + edge.h / 2.0;
        page.drag_between(
            pencere_view::Point::new(edge.x + edge.w / 2.0, at),
            pencere_view::Point::new(1.0, at),
        )?;
        page.press(keys)?;
        page.advance(1.0)?;
        page.settle()?;
        let png = page.shot_in(&fonts, Palette::Dark).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    // A project of repositories, in its folders: the panel as a list and under folders, and the tree.
    for (name, keys, size, palette) in [
        (
            "repos-list-dark",
            vec!["cmd+shift+g"],
            (1280.0, 800.0),
            Palette::Dark,
        ),
        (
            "repos-tree-dark",
            vec!["cmd+shift+g", "t"],
            (1280.0, 800.0),
            Palette::Dark,
        ),
        (
            "repos-list-light",
            vec!["cmd+shift+g"],
            (1280.0, 800.0),
            Palette::Light,
        ),
        ("repos-list-80x24", vec!["cmd+shift+g"], PANE, Palette::Dark),
        (
            "repos-files-dark",
            vec!["cmd+shift+e"],
            (1280.0, 800.0),
            Palette::Dark,
        ),
    ] {
        let project = lantern();
        project.write("api/src/routes/orders.ts", "export const orders = [];\n");
        project.write("api/src/server.ts", "a\nb\nc\n");
        project.write("api/README.md", "# api\n");
        project.write("site/index.html", "<!doctype html>\n");
        project.write("site/.gitignore", "dist/\n");
        project.write("worker/main.ts", "a\n");
        project.write("libs/kit/kit.ts", "a\n");
        for (dir, branch) in [
            ("api", "main"),
            ("site", "redesign"),
            ("worker", "main"),
            ("libs/kit", "main"),
        ] {
            let folder = project.root().join(dir);
            let git = |args: &[&str]| {
                let status = Command::new("git")
                    .args(args)
                    .current_dir(&folder)
                    .env("GIT_AUTHOR_NAME", "textpen")
                    .env("GIT_AUTHOR_EMAIL", "textpen@example.com")
                    .env("GIT_COMMITTER_NAME", "textpen")
                    .env("GIT_COMMITTER_EMAIL", "textpen@example.com")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_SYSTEM", "/dev/null")
                    .status()
                    .expect("git");
                assert!(status.success(), "git {args:?}");
            };
            git(&["init", "-q", "-b", branch]);
            git(&["add", "."]);
            git(&["commit", "-qm", "one"]);
        }
        project.write("api/src/server.ts", "a\nB\nc\nd\ne\n");
        project.write(
            "api/src/routes/refunds.ts",
            "export const refunds = [];\nexport const more = 1;\n",
        );
        std::fs::remove_file(project.root().join("api/README.md")).expect("removed");
        project.write(
            "site/index.html",
            "<!doctype html>\n<title>Lantern</title>\n",
        );
        project.write("site/dist/bundle.js", "x\n");
        let mut page = open(&project, &["README.md"], size, &fonts)?;
        for key in &keys {
            page.press(key)?;
            page.settle()?;
        }
        wait(&mut page, |page| shows(page, "repos"))?;
        if name == "repos-files-dark" {
            for dir in ["api", "site", "site/dist"] {
                wait(&mut page, |page| {
                    page.bounds(&format!("row:{dir}")).is_some()
                })?;
                page.click(&format!("row:{dir}"))?;
            }
        }
        page.advance(1.0)?;
        page.settle()?;
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    // The tree, with what git ignores quiet all the way down.
    for (name, palette) in [
        ("tree-ignored-dark", Palette::Dark),
        ("tree-ignored-light", Palette::Light),
    ] {
        let project = lantern();
        project.write(".gitignore", "out/\n*.log\n");
        project.write("out/deep/inner.txt", "x\n");
        project.write("logs/today.log", "x\n");
        project.write("logs/kept.txt", "x\n");
        with_git(&project);
        let mut page = open(&project, &[], (1280.0, 800.0), &fonts)?;
        page.press("cmd+shift+e")?;
        for dir in ["out", "out/deep", "logs"] {
            wait(&mut page, |page| {
                page.bounds(&format!("row:{dir}")).is_some()
            })?;
            page.click(&format!("row:{dir}"))?;
        }
        wait(&mut page, |page| {
            page.bounds("row:out/deep/inner.txt").is_some()
        })?;
        page.advance(1.0)?;
        page.settle()?;
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    }
    Ok(())
}

/// The pictures of what the person sees as files change hands: files dragged over a folder, two
/// that are on their way (one with the pointer on it), and the question of a name that is taken.
#[test]
#[ignore = "shots: draws the page in the fonts of this machine into docs/; make shots"]
fn the_pictures_of_the_uploads() -> Result<(), Error> {
    use pencere_view::FileSource;
    let fonts = fonts();
    let patience = std::time::Duration::from_secs(4);
    let size = (900.0, 560.0);
    let opened = |project: &Project| -> Result<_, Error> {
        let session = project.session(&["src/server.ts"]);
        let double = moving(&session);
        let mut page = open_quick_in(&session, size, Some(&fonts), patience)?;
        // The file in front has its folder open: the folder beside it is opened.
        wait_through(&mut page, |page| page.bounds("row:src/lib").is_some())?;
        page.press("cmd+shift+e")?;
        page.click("row:src/lib")?;
        wait_through(&mut page, |page| {
            page.bounds("row:src/lib/format.ts").is_some()
        })?;
        Ok((page, double))
    };

    // Dragged over a folder: the folder is marked, and named.
    let project = lantern();
    with_git(&project);
    let (mut page, _) = opened(&project)?;
    over_row(&mut page, "row:src/lib", 2)?;
    page.advance(0.3)?;
    save(&page, &fonts, Palette::Dark, "upload-over.png");
    page.drag_files_away()?;

    // Two files on their way, the pointer on the second: how far each has got, and Cancel.
    let (mut page, double) = opened(&project)?;
    let bytes = vec![7_u8; 100];
    double.give(&handle(1), &bytes);
    double.give(&handle(2), &bytes);
    *double.piece.lock().expect("a lock") = 21;
    double.hold_at(42);
    give(
        &mut page,
        "row:src/lib",
        FileSource::Drop,
        &[("product-photo.png", &bytes), ("notes.txt", &bytes)],
    )?;
    wait_through(&mut page, |page| {
        shows(page, "product-photo.png") && shows(page, "notes.txt") && shows(page, "42%")
    })?;
    through(page.hover("row:src/lib/notes.txt"))?;
    wait_through(&mut page, |page| shows(page, "Cancel"))?;
    page.advance(0.3).ok();
    save(&page, &fonts, Palette::Dark, "upload-going.png");
    double.go_on();
    wait_through(&mut page, |page| !shows(page, "42%"))?;

    // A name that is taken: one question, Replace or Keep Both.
    let project = lantern();
    project.write("src/lib/product-photo.png", "the one that is there");
    with_git(&project);
    let (mut page, double) = opened(&project)?;
    double.give(&handle(1), &bytes);
    give(
        &mut page,
        "row:src/lib",
        FileSource::Drop,
        &[("product-photo.png", &bytes)],
    )?;
    wait_through(&mut page, |page| shows(page, "Keep Both"))?;
    page.advance(0.4).ok();
    save(&page, &fonts, Palette::Dark, "upload-asks.png");

    // A picture pasted in a Markdown file: written into it at once, and the preview holds its place.
    let project = lantern();
    with_git(&project);
    let session = project.session(&["README.md"]);
    let double = moving(&session);
    // The preview reads the text again after 0.2 s of the page's clock, which moves a frame a
    // round of the host's patience: a short one.
    let mut page = open_quick_in(
        &session,
        (1100.0, 680.0),
        Some(&fonts),
        std::time::Duration::from_millis(1000),
    )?;
    let art = lantern_art();
    double.give(&handle(1), &art);
    *double.piece.lock().expect("a lock") = art.len() / 3;
    double.hold_at(u64::try_from(art.len() / 3).expect("a size"));
    through(page.press("cmd+shift+v"))?;
    // The caret is on the empty line under the heading: the picture is a paragraph of its own.
    through(page.press("down"))?;
    let editor = crate::page_tests::shown(&page).expect("an editor");
    let at = page.bounds(&editor).expect("the editor");
    page.page_mut().give(
        pencere_view::Point::new(at.x + 80.0, at.y + 200.0),
        FileSource::Paste,
        vec![pencere_view::FileEntry {
            handle: Some(handle(1)),
            name: "image.png".to_owned(),
            size: art.len() as u64,
            media_type: "image/png".to_owned(),
            path: None,
            folder: false,
        }],
    );
    through(page.settle())?;
    wait_through(&mut page, |page| {
        page.bounds("upload-placeholder:1").is_some()
    })?;
    wait_through(&mut page, |page| shows(page, "33%"))?;
    page.advance(0.3).ok();
    save(&page, &fonts, Palette::Light, "upload-paste.png");

    // An 80×24 pane, in a light terminal: an upload that has stalled, the end of its row faded where
    // it does not fit.
    let project = lantern();
    with_git(&project);
    let session = project.session(&["src/server.ts"]);
    let double = moving(&session);
    let mut page = open_quick_in(
        &session,
        PANE,
        Some(&fonts),
        std::time::Duration::from_millis(1500),
    )?;
    page.press("cmd+shift+e")?;
    wait_through(&mut page, |page| page.bounds("row:src/lib").is_some())?;
    double.give(&handle(1), &bytes);
    *double.piece.lock().expect("a lock") = 50;
    double.hold_at(50);
    give(
        &mut page,
        "row:src/lib",
        FileSource::Drop,
        &[("a-rather-long-name-for-a-report.txt", &bytes)],
    )?;
    wait_through(&mut page, |page| shows(page, "50%"))?;
    pass(&mut page, 32.0)?;
    wait_through(&mut page, |page| shows(page, "Upload stalled"))?;
    through(page.hover("row:src/lib/a-rather-long-name-for-a-report.txt"))?;
    wait_through(&mut page, |page| shows(page, "Cancel"))?;
    through(page.tick())?;
    save(&page, &fonts, Palette::Light, "upload-stalled-80x24.png");
    Ok(())
}

// --- A PDF's tab ----------------------------------------------------------------------------------

/// The paragraphs of the PDF the pictures show: a specification's pages, with a heading to each.
const SPEC: [(&str, [&str; 3]); 4] = [
    (
        "8.6 Bundles",
        [
            "A bundle is the set of files a view is made of: its entry, which the host reads first, and the files the entry names. The host keeps them by content, so that a file it holds already crosses the stream once, whichever view or open it is for.",
            "The manifest lists every file of the bundle with its path, its digest (sha256) and its length in bytes. A host asks for the files it does not hold, and the program sends only those.",
            "A bundle is served to the view as the files of a page are served: the entry as the document and the other files at their paths, each with the type of its kind.",
        ],
    ),
    (
        "8.7 The entry",
        [
            "The entry of a bundle is the view's own file, and its media type is that of the kind: an HTML document for a page, a script for lua. The host does not decide the kind from the entry, and it does not guess.",
            "A view that names a file the manifest does not list is given nothing for it. The page is told, as a person who wrote it should be, and the view is not closed.",
            "The entry is read once for each open of the view. A host that holds the bundle reads the same entry from its own store, and asks the program for nothing.",
        ],
    ),
    (
        "8.8 Attached files",
        [
            "A program may add files to the view that runs, beside its bundle, for the view to draw: the pictures of a project, of a document, of a preview. They are kept by content as the files of a bundle are, so a file that the host still holds crosses the stream once.",
            "The program sends an attach: the files, each by its digest (sha256) and its length in bytes (size). The host answers every attach with a need, whose re is the attach's id, and the program sends the files that are named in it.",
            "A file attached is the view's for as long as the view runs, and is let go when the program says so. The view names it as it names a file of its bundle, by the name the program gives it.",
        ],
    ),
    (
        "8.9 Detached files",
        [
            "The program sends a detach for a file it no longer holds for the view: the host forgets the file, and so does every image of the view that names it. A view that still draws it draws nothing there.",
            "Files that were attached for a view stay with the host while another view holds them, and go when the last of them lets go. Nothing of the kind is told to the program.",
            "A program that ends takes its attached files with it. The view that ran it is closed, or goes on without them.",
        ],
    ),
];

/// Breaks `words` into lines of at most `width` characters.
fn broken(words: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in words.split(' ') {
        let line = lines.last_mut().expect("a line");
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(word.to_owned());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

/// A specification of four pages, 612 × 792 points, in Times: a running head, a heading and three
/// paragraphs to each, and its page number under.
fn spec_pdf() -> Vec<u8> {
    let pages: Vec<(f32, f32, Vec<Run>)> = SPEC
        .iter()
        .enumerate()
        .map(|(number, (heading, paragraphs))| {
            let run = |font, size, x, y, text: &str| Run {
                font,
                size,
                x,
                y,
                text: text.to_owned(),
            };
            let mut runs = vec![
                run(4, 8.5, 72.0, 740.0, "pencere protocol, draft"),
                run(4, 8.5, 470.0, 740.0, "8 Bundles"),
                run(2, 17.0, 72.0, 690.0, heading),
            ];
            let mut y = 660.0;
            for paragraph in paragraphs {
                for line in broken(paragraph, 88) {
                    runs.push(run(1, 11.0, 72.0, y, &line));
                    y -= 14.5;
                }
                y -= 10.0;
            }
            runs.push(run(4, 8.5, 300.0, 54.0, &(number + 14).to_string()));
            (612.0, 792.0, runs)
        })
        .collect();
    pdf_of_runs(&pages)
}

/// The page opened on `bytes` as `docs/pencere-spec.pdf` in front, drawn; the project is the caller's to
/// keep for as long as the page is looked at.
fn open_pdf_pictured(
    bytes: &[u8],
    size: (f32, f32),
    fonts: &Fonts,
) -> Result<(Project, Page), Error> {
    let project = lantern();
    project.write_bytes("docs/pencere-spec.pdf", bytes);
    with_git(&project);
    let session = project.session(&["docs/pencere-spec.pdf", "README.md", "src/server.ts"]);
    recording(&session);
    let program = InProcess::new(options(&session, &[], &Arc::new(Mutex::new(None))));
    let mut page = open_holding(program, size, Some(fonts), 0.0, &[bytes])?;
    wait(&mut page, |page| !pages_drawn(page).is_empty())?;
    page.advance(1.5)?;
    Ok((project, page))
}

/// A PDF in its tab for the docs: the pages one under the other, a search open with its marks on them.
#[test]
#[ignore = "shots: draws the page in the fonts of this machine into docs/; make shots"]
fn the_picture_of_a_pdf_in_the_docs() -> Result<(), Error> {
    let fonts = fonts();
    let (_project, mut page) = open_pdf_pictured(&spec_pdf(), (1280.0, 800.0), &fonts)?;
    page.press("ctrl+g")?;
    page.type_text("3")?;
    page.press("enter")?;
    page.advance(0.5)?;
    page.press("cmd+f")?;
    page.type_text("attach")?;
    wait(&mut page, |page| shows(page, " of "))?;
    page.press("enter")?;
    page.press("enter")?;
    page.advance(1.0)?;
    // The bar's field is the host's to draw, not the shot's.
    save(&page, &fonts, Palette::Dark, "pdf.png");
    Ok(())
}

/// A PDF in its tab, in a dark and a light terminal, wide and in an 80×24 pane, with a search, a
/// selection, no words and too large, for the eye (`target/shots/`).
#[test]
#[ignore = "shots: draws the page in the fonts of this machine into target/shots/"]
fn the_pictures_of_a_pdf() -> Result<(), Error> {
    let fonts = fonts();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shots");
    std::fs::create_dir_all(&out).expect("the folder");
    let write = |page: &Page, palette: Palette, name: &str| {
        let png = page.shot_in(&fonts, palette).expect("a picture");
        std::fs::write(out.join(format!("{name}.png")), png).expect("the picture written");
    };
    for (name, size, palette) in [
        ("pdf-dark-wide", (1280.0, 800.0), Palette::Dark),
        ("pdf-light-wide", (1280.0, 800.0), Palette::Light),
        ("pdf-dark-80x24", PANE, Palette::Dark),
        ("pdf-light-80x24", PANE, Palette::Light),
    ] {
        let (_project, mut page) = open_pdf_pictured(&spec_pdf(), size, &fonts)?;
        write(&page, palette, name);
        if size == PANE {
            continue;
        }
        // The second page in view, a search open and a selection made in it.
        page.press("ctrl+g")?;
        page.type_text("3")?;
        page.press("enter")?;
        page.advance(0.5)?;
        page.press("cmd+f")?;
        page.type_text("attach")?;
        wait(&mut page, |page| shows(page, " of "))?;
        page.advance(1.0)?;
        write(&page, palette, &format!("{name}-find"));
        page.press("escape")?;
        let heading = page.bounds("pdf-page:3").expect("page 3");
        let at = pencere_view::Point::new(heading.x + 110.0, heading.y + 180.0);
        page.pointer(pencere_view::Input::Press {
            at,
            button: pencere_view::Button::Left,
            mods: pencere_view::Mods::default(),
            count: 1,
        })?;
        page.pointer(pencere_view::Input::Press {
            at,
            button: pencere_view::Button::Left,
            mods: pencere_view::Mods::default(),
            count: 3,
        })?;
        page.pointer(pencere_view::Input::Release {
            at,
            button: pencere_view::Button::Left,
        })?;
        page.advance(0.5)?;
        write(&page, palette, &format!("{name}-select"));
    }
    // No words: the bar says so.
    let (_project, mut page) = open_pdf_pictured(
        &pdf_of_pages(&[(612.0, 792.0), (612.0, 792.0)]),
        (1280.0, 800.0),
        &fonts,
    )?;
    page.press("cmd+f")?;
    page.type_text("total")?;
    page.advance(0.5)?;
    write(&page, Palette::Dark, "pdf-no-text");
    // Too large to preview.
    let mut big = pdf_of_pages(&[(612.0, 792.0)]);
    big.resize(41 * 1024 * 1024, b' ');
    let project = lantern();
    project.write_bytes("docs/scans-2024.pdf", &big);
    let mut page = open(&project, &["README.md"], (1280.0, 800.0), &fonts)?;
    page.click("row:docs")?;
    wait(&mut page, |page| {
        page.bounds("row:docs/scans-2024.pdf").is_some()
    })?;
    page.click("row:docs/scans-2024.pdf")?;
    wait(&mut page, |page| shows(page, "Too large to preview"))?;
    page.advance(0.5)?;
    write(&page, Palette::Dark, "pdf-too-large");
    Ok(())
}
