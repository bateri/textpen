//! The repositories of a project: the one it is in, or, where it is in none, those in its folders, each
//! with its own git and what that git said. A file is of the nearest one above it, and the page is told
//! each by paths from the project's root, so that nothing else has to know where a repository begins.
//!
//! They are looked for two levels down, through no link, no folder whose name begins with a dot and none
//! of what tools fill (`node_modules`, `target`), whose clones are not the person's work. A project in a
//! repository has that one alone, as what is in a repository's folders is its own to say.
//!
//! Plain values in and out: this file knows nothing of the page.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::git::{Git, GitInfo};
use super::tree::SKIPPED;

/// How many levels of folders are looked in.
const LEVELS: usize = 2;
/// The most repositories a project has: what lies past them is as a folder with no git.
const MOST: usize = 64;
/// How long what was found is used before the folders are looked in again.
const SCAN_AGE: Duration = Duration::from_secs(10);
/// How often a repository the person is not in is asked, and how many of them at one asking: asked all
/// together they would start a dozen gits at a time, every time.
const OTHERS_AGE: Duration = Duration::from_secs(10);
const OTHERS_AT_ONCE: usize = 3;
/// How many are asked at the same time when all are, as the page starts.
const AT_THE_SAME_TIME: usize = 8;

/// One repository of a project.
pub struct Repo {
    /// Its folder from the project's root, none where the project is in it.
    pub root: String,
    pub git: Git,
    /// When it was last asked.
    asked: Mutex<Option<Instant>>,
}

impl Repo {
    fn new(root: String, folder: &str) -> Self {
        Self {
            root,
            git: Git::new(folder),
            asked: Mutex::new(None),
        }
    }

    /// What git says of it, by paths from the project's root. What it said less than `max_age` ago is
    /// not asked again.
    pub fn status(&self, max_age: Duration) -> Option<GitInfo> {
        *self.asked.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        self.git.status(max_age).map(|info| info.under(&self.root))
    }

    /// A path of this repository as the project names it.
    pub fn outer(&self, path: &str) -> String {
        outer(&self.root, path)
    }

    fn last_asked(&self) -> Option<Instant> {
        *self.asked.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A path under the folder `root` of the project, from the project's root.
pub fn outer(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}/{path}")
    }
}

/// What was found.
struct Found {
    repos: Vec<Arc<Repo>>,
    /// The project is in a repository: there is nothing to look for.
    whole: bool,
    at: Instant,
}

/// The repositories of the project at `root`, found when they are first asked for.
pub struct Repos {
    root: String,
    found: Mutex<Option<Found>>,
}

impl Repos {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_owned(),
            found: Mutex::new(None),
        }
    }

    fn found(&self) -> MutexGuard<'_, Option<Found>> {
        let mut found = self.found.lock().unwrap_or_else(PoisonError::into_inner);
        if found.is_none() {
            let own = Repo::new(String::new(), &self.root);
            *found = Some(if own.git.is_repository() {
                Found {
                    repos: vec![Arc::new(own)],
                    whole: true,
                    at: Instant::now(),
                }
            } else {
                Found {
                    repos: self.look(&[]),
                    whole: false,
                    at: Instant::now(),
                }
            });
        }
        found
    }

    /// The repositories in the folders, in the order of their folders: those already known are kept
    /// with what they said, and a folder with a `.git` git does not take is not one.
    fn look(&self, known: &[Arc<Repo>]) -> Vec<Arc<Repo>> {
        let mut roots = Vec::new();
        folders_with_git(Path::new(&self.root), "", LEVELS, &mut roots);
        roots.sort();
        roots.truncate(MOST);
        roots
            .into_iter()
            .filter_map(|root| {
                if let Some(repo) = known.iter().find(|repo| repo.root == root) {
                    return Some(Arc::clone(repo));
                }
                let folder = Path::new(&self.root).join(&root);
                let repo = Repo::new(root, folder.to_str()?);
                repo.git.is_repository().then(|| Arc::new(repo))
            })
            .collect()
    }

    /// Looks in the folders again where it is time to: a repository may have been cloned or removed.
    pub fn look_again(&self) {
        let mut guard = self.found();
        let Some(found) = guard.as_mut() else {
            return;
        };
        if !found.whole && found.at.elapsed() >= SCAN_AGE {
            found.repos = self.look(&found.repos);
            found.at = Instant::now();
        }
    }

    pub fn list(&self) -> Vec<Arc<Repo>> {
        self.found()
            .as_ref()
            .map_or_else(Vec::new, |found| found.repos.clone())
    }

    /// The repository `path` (from the project's root) is in, and the path from its own root.
    pub fn of(&self, path: &str) -> Option<(Arc<Repo>, String)> {
        let list = self.list();
        if let [only] = list.as_slice()
            && only.root.is_empty()
        {
            return Some((Arc::clone(only), path.to_owned()));
        }
        list.into_iter().find_map(|repo| {
            let inner = path
                .strip_prefix(repo.root.as_str())?
                .strip_prefix('/')?
                .to_owned();
            Some((repo, inner))
        })
    }

    /// Those to ask now: the one the file in front is in, at every asking, and those that have waited
    /// longest of the others.
    pub fn due(&self, front: Option<&str>) -> Vec<Arc<Repo>> {
        let list = self.list();
        let front = front.and_then(|path| self.of(path)).map(|(repo, _)| repo);
        let mut others: Vec<Arc<Repo>> = list
            .into_iter()
            .filter(|repo| !front.as_ref().is_some_and(|front| Arc::ptr_eq(front, repo)))
            .filter(|repo| {
                repo.root.is_empty()
                    || repo
                        .last_asked()
                        .is_none_or(|asked| asked.elapsed() >= OTHERS_AGE)
            })
            .collect();
        others.sort_by_key(|repo| repo.last_asked());
        others.truncate(OTHERS_AT_ONCE);
        front.into_iter().chain(others).collect()
    }

    /// What each says now, by the same order, for a page that is starting.
    pub fn all(&self) -> Vec<(String, Option<GitInfo>)> {
        let list = self.list();
        let mut said = Vec::with_capacity(list.len());
        for some in list.chunks(AT_THE_SAME_TIME) {
            thread::scope(|scope| {
                let asking: Vec<_> = some
                    .iter()
                    .map(|repo| scope.spawn(|| repo.status(Duration::ZERO)))
                    .collect();
                for (repo, answer) in some.iter().zip(asking) {
                    said.push((repo.root.clone(), answer.join().ok().flatten()));
                }
            });
        }
        said
    }

    /// Forgets what each said: the files changed.
    pub fn forget(&self) {
        for repo in self.list() {
            repo.git.forget();
        }
    }
}

/// The folders under `dir` (`from`, from the project's root) that hold a `.git`, `levels` levels down.
fn folders_with_git(dir: &Path, from: &str, levels: usize, found: &mut Vec<String>) {
    let Ok(items) = fs::read_dir(dir) else {
        return;
    };
    for item in items.flatten() {
        let Ok(name) = item.file_name().into_string() else {
            continue;
        };
        // A link is not followed: it may go anywhere, the project's own parent among them.
        if !item.file_type().is_ok_and(|kind| kind.is_dir())
            || name.starts_with('.')
            || SKIPPED.contains(&name.as_str())
        {
            continue;
        }
        let path = item.path();
        let root = outer(from, &name);
        // A directory, or the file a worktree of another checkout has.
        if fs::symlink_metadata(path.join(".git")).is_ok() {
            found.push(root);
        } else if levels > 1 {
            folders_with_git(&path, &root, levels - 1, found);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git_in(dir: &Path, args: &[&str]) {
        let made = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(made.status.success(), "{args:?}: {made:?}");
    }

    fn repository_at(root: &Path, folder: &str) {
        let dir = root.join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "a\n").unwrap();
        git_in(&dir, &["init", "-q"]);
        git_in(&dir, &["add", "."]);
        git_in(&dir, &["commit", "-qm", "one"]);
    }

    fn project() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        (dir, root)
    }

    fn roots(repos: &Repos) -> Vec<String> {
        repos.list().iter().map(|repo| repo.root.clone()).collect()
    }

    /// Those in the folders, two levels down; not what is past that, what a tool made, what a dot hides,
    /// what a link leads to, what is inside another repository or a `.git` git does not take.
    #[test]
    fn the_repositories_are_those_in_the_folders_that_are_the_persons() {
        let (_dir, root) = project();
        repository_at(&root, "alpha");
        repository_at(&root, "group/deep");
        repository_at(&root, "group/deeper/still");
        repository_at(&root, "alpha/inner");
        repository_at(&root, "node_modules/lib");
        repository_at(&root, "target/clone");
        repository_at(&root, ".cache/copy");
        std::os::unix::fs::symlink(root.join("alpha"), root.join("linked")).unwrap();
        fs::create_dir_all(root.join("broken/.git")).unwrap();
        fs::create_dir_all(root.join("plain")).unwrap();
        let repos = Repos::new(root.to_str().unwrap());
        assert_eq!(roots(&repos), ["alpha", "group/deep"]);
    }

    /// A project in a repository has that one, whatever is in its folders.
    #[test]
    fn a_project_in_a_repository_has_that_one_alone() {
        let (_dir, root) = project();
        repository_at(&root, "");
        repository_at(&root, "vendored");
        let repos = Repos::new(root.to_str().unwrap());
        assert_eq!(roots(&repos), [""]);
        let (repo, inner) = repos.of("vendored/a.txt").unwrap();
        assert_eq!((repo.root.as_str(), inner.as_str()), ("", "vendored/a.txt"));
    }

    /// A worktree has a file for its `.git`, and is a repository as any other.
    #[test]
    fn a_worktree_of_another_checkout_is_one() {
        let (_dir, root) = project();
        repository_at(&root, "main");
        git_in(
            &root.join("main"),
            &["worktree", "add", "-q", "../other", "-b", "other"],
        );
        let repos = Repos::new(root.to_str().unwrap());
        assert_eq!(roots(&repos), ["main", "other"]);
    }

    /// A file is of the repository above it and of no other that begins alike; what is in none is of none.
    /// What is told is by paths from the project's root.
    #[test]
    fn a_file_is_of_the_repository_above_it() {
        let (_dir, root) = project();
        repository_at(&root, "app");
        repository_at(&root, "app-two");
        fs::write(root.join("app/b.txt"), "b\n").unwrap();
        fs::write(root.join("loose.txt"), "b\n").unwrap();
        let repos = Repos::new(root.to_str().unwrap());
        let (repo, inner) = repos.of("app/b.txt").unwrap();
        assert_eq!((repo.root.as_str(), inner.as_str()), ("app", "b.txt"));
        assert_eq!(repos.of("app-two/a.txt").unwrap().0.root, "app-two");
        assert!(repos.of("loose.txt").is_none());
        assert!(repos.of("app").is_none());
        let said = repos.all();
        let (found, info) = &said[0];
        assert_eq!(found, "app");
        let info = info.as_ref().unwrap();
        assert!(info.changes.contains_key("app/b.txt"), "{:?}", info.changes);
        assert_eq!(info.stats.get("app/b.txt"), Some(&[1, 0]));
    }

    /// A repository cloned while the project is open is found at a later look, and one removed goes.
    #[test]
    fn what_is_in_the_folders_is_looked_for_again() {
        let (_dir, root) = project();
        repository_at(&root, "alpha");
        let repos = Repos::new(root.to_str().unwrap());
        assert_eq!(roots(&repos), ["alpha"]);
        repository_at(&root, "beta");
        repos.look_again();
        assert_eq!(roots(&repos), ["alpha"], "not yet time");
        repos.found.lock().unwrap().as_mut().unwrap().at -= SCAN_AGE;
        repos.look_again();
        assert_eq!(roots(&repos), ["alpha", "beta"]);
        fs::remove_dir_all(root.join("alpha")).unwrap();
        repos.found.lock().unwrap().as_mut().unwrap().at -= SCAN_AGE;
        repos.look_again();
        assert_eq!(roots(&repos), ["beta"]);
    }

    /// The one in front is asked at every asking; of the others only so many, those that waited longest,
    /// and none of them again until it is its turn. A project in one repository has that one every time.
    #[test]
    fn the_repository_in_front_is_asked_every_time_and_the_others_in_turn() {
        let (_dir, root) = project();
        for name in ["a", "b", "c", "d", "e"] {
            repository_at(&root, name);
        }
        let repos = Repos::new(root.to_str().unwrap());
        let names = |due: Vec<Arc<Repo>>| -> Vec<String> {
            due.iter()
                .map(|repo| {
                    *repo.asked.lock().unwrap() = Some(Instant::now());
                    repo.root.clone()
                })
                .collect()
        };
        let first = names(repos.due(Some("c/a.txt")));
        assert_eq!(first, ["c", "a", "b", "d"]);
        let second = names(repos.due(Some("c/a.txt")));
        assert_eq!(second, ["c", "e"]);
        let third = names(repos.due(Some("c/a.txt")));
        assert_eq!(third, ["c"]);
        for repo in repos.list() {
            let long_ago = Instant::now() - OTHERS_AGE;
            *repo.asked.lock().unwrap() = Some(long_ago);
        }
        assert_eq!(names(repos.due(None)).len(), OTHERS_AT_ONCE);

        let (_dir, root) = project();
        repository_at(&root, "");
        let repos = Repos::new(root.to_str().unwrap());
        assert_eq!(names(repos.due(None)), [""]);
        assert_eq!(names(repos.due(None)), [""]);
    }
}
