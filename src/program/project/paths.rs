//! Paths by their text alone, `/`-separated: nothing here asks the disk, so a file that is not there
//! yet has a path, and a link is not followed.

/// Whether `path` starts at the top of the disk.
pub fn is_absolute(path: &str) -> bool {
    path.starts_with('/')
}

/// The path with its `.` and `..` parts worked out, and no empty ones; "." for nothing at all.
pub fn normalize(path: &str) -> String {
    let absolute = is_absolute(path);
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        match part {
            "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

/// `path` made absolute from `base` (an absolute path), as `path.resolve(base, path)` does.
pub fn resolve(base: &str, path: &str) -> String {
    if is_absolute(path) {
        normalize(path)
    } else {
        normalize(&format!("{base}/{path}"))
    }
}

/// The parts joined, and worked out.
pub fn join(first: &str, second: &str) -> String {
    normalize(&format!("{first}/{second}"))
}

/// The directory a path is in: "." for a name alone, "/" for one at the top.
pub fn dirname(path: &str) -> String {
    match path.rfind('/') {
        None => ".".to_owned(),
        Some(0) => "/".to_owned(),
        Some(at) => path.get(..at).unwrap_or_default().to_owned(),
    }
}

/// The last part of a path, without the `/` that may end it.
pub fn basename(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or_default()
}

/// Where `to` is from `from`, both absolute and worked out: "" for the same place, and a path that
/// starts with ".." for one that is not below it.
pub fn relative(from: &str, to: &str) -> String {
    if from == to {
        return String::new();
    }
    let prefix = if from == "/" {
        "/".to_owned()
    } else {
        format!("{from}/")
    };
    if let Some(rest) = to.strip_prefix(&prefix) {
        return rest.to_owned();
    }
    let a: Vec<&str> = from.split('/').filter(|part| !part.is_empty()).collect();
    let b: Vec<&str> = to.split('/').filter(|part| !part.is_empty()).collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out: Vec<&str> = vec![".."; a.len() - common];
    out.extend(b.iter().skip(common));
    out.join("/")
}

/// Whether `path` is `dir` or a path under it.
pub fn is_within(path: &str, dir: &str) -> bool {
    path == dir
        || dir == "/"
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_worked_out_by_its_text() {
        assert_eq!(normalize("a/./b/../c//d/"), "a/c/d");
        assert_eq!(normalize("../a"), "../a");
        assert_eq!(normalize("/../a"), "/a");
        assert_eq!(normalize(""), ".");
        assert_eq!(resolve("/root", "x/y"), "/root/x/y");
        assert_eq!(resolve("/root", ""), "/root");
        assert_eq!(resolve("/root", "/elsewhere/./z"), "/elsewhere/z");
        assert_eq!(join("/root", "a"), "/root/a");
        assert_eq!(dirname("/root/a.txt"), "/root");
        assert_eq!(dirname("/a.txt"), "/");
        assert_eq!(dirname("a.txt"), ".");
        assert_eq!(basename("/root/dir/"), "dir");
        assert_eq!(basename("/"), "");
    }

    #[test]
    fn a_relative_path_climbs_out_with_dots() {
        assert_eq!(relative("/root", "/root"), "");
        assert_eq!(relative("/root", "/root/a/b"), "a/b");
        assert_eq!(relative("/root", "/rooted/a"), "../rooted/a");
        assert_eq!(relative("/root/x", "/root/y"), "../y");
        assert_eq!(relative("/", "/a"), "a");
    }

    #[test]
    fn a_path_is_within_a_directory_only_below_it() {
        assert!(is_within("/root", "/root"));
        assert!(is_within("/root/a", "/root"));
        assert!(!is_within("/rooted", "/root"));
        assert!(is_within("/anything", "/"));
    }
}
