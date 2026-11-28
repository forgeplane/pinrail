//! The folders the person's agents worked in before Pinrail saw them, read
//! from what each agent keeps on this computer. A new session can then be
//! offered a folder on the first run, when Pinrail has seen none itself.
//!
//! Each agent keeps its sessions its own way, so each has a reader here.
//! A reader looks at the newest few entries only, and one that finds
//! nothing, or a layout it does not know, answers nothing.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// How many of an agent's newest entries a reader looks at.
const NEWEST: usize = 8;
/// The most of a session file read to find its folder. Claude Code names
/// it within the first kilobyte, and the first line of a Codex session,
/// which holds its instructions too, is some twenty.
const HEAD: u64 = 256 * 1024;

/// A folder an agent worked in, and when it last did.
#[derive(Debug, Clone, PartialEq)]
pub struct Used {
    pub folder: String,
    /// the agent whose history names it, by its id
    pub agent: &'static str,
    pub used_at: DateTime<Utc>,
}

/// The folders the agents under `home` worked in lately that are still
/// there, the most recent first, at most `limit` of them. A folder two
/// agents worked in is its most recent user's. Scratch folders, the home
/// folder itself and the root are left out: they are no project's.
pub fn recent_folders(home: &Path, limit: usize) -> Vec<Used> {
    // a home folder that is itself in a temporary folder, as a test's is,
    // has its projects there
    recent_where(home, limit, &|folder| {
        !is_scratch(folder) || under(folder, home)
    })
}

/// [`recent_folders`], keeping the folders `keep` says to.
fn recent_where(home: &Path, limit: usize, keep: &dyn Fn(&Path) -> bool) -> Vec<Used> {
    let mut found: Vec<Used> = Vec::new();
    let readers: [(&'static str, Vec<(String, SystemTime)>); 5] = [
        (
            "claude-code",
            session_folders(&home.join(".claude/projects")),
        ),
        ("codex", codex(&home.join(".codex/sessions"))),
        ("opencode", opencode(&home.join(".local/share/opencode"))),
        ("grok", grok(&home.join(".grok/sessions"))),
        ("pi", session_folders(&home.join(".pi/agent/sessions"))),
    ];
    for (agent, folders) in readers {
        for (folder, at) in folders {
            let path = Path::new(&folder);
            if !path.is_absolute() || path.parent().is_none() || same_folder(path, home) {
                continue;
            }
            if !path.is_dir() || !keep(path) {
                continue;
            }
            let used_at = DateTime::<Utc>::from(at);
            match found
                .iter_mut()
                .find(|used| same_folder(Path::new(&used.folder), path))
            {
                Some(used) if used.used_at < used_at => {
                    used.agent = agent;
                    used.used_at = used_at;
                }
                Some(_) => {}
                None => found.push(Used {
                    folder,
                    agent,
                    used_at,
                }),
            }
        }
    }
    found.sort_by_key(|used| std::cmp::Reverse(used.used_at));
    found.truncate(limit);
    found
}

/// A folder's path as it compares on Windows: in its long form, as a
/// temporary folder can come as ALICE~1, as far as it is there to read,
/// so that a folder not made yet compares as it will be; without the
/// \\?\ of a long path, a share's as \\server\share; in lower case,
/// with \ between its parts.
#[cfg(windows)]
fn comparable(path: &Path) -> String {
    let long = path
        .ancestors()
        .find_map(|there| {
            let rest = path.strip_prefix(there).ok()?;
            let resolved = there.canonicalize().ok()?;
            Some(if rest.as_os_str().is_empty() {
                resolved
            } else {
                resolved.join(rest)
            })
        })
        .unwrap_or_else(|| path.to_path_buf());
    let text = long.to_string_lossy();
    let text = match text.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => text.strip_prefix(r"\\?\").unwrap_or(&text).to_string(),
    };
    text.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Whether two paths name one folder: on Windows in any capitals, with
/// either separator.
pub(crate) fn same_folder(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        comparable(a) == comparable(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// Whether `folder` is `base` or inside it, compared as [`same_folder`]
/// compares.
pub(crate) fn under(folder: &Path, base: &Path) -> bool {
    #[cfg(windows)]
    {
        let (folder, base) = (comparable(folder), comparable(base));
        folder == base || folder.starts_with(&format!("{base}\\"))
    }
    #[cfg(not(windows))]
    {
        folder.starts_with(base)
    }
}

/// Whether `folder` is under a temporary folder, where an agent's work is
/// a test or a one-off.
pub(crate) fn is_scratch(folder: &Path) -> bool {
    [
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/private/var/folders",
        "/var/tmp",
    ]
    .iter()
    .any(|scratch| folder.starts_with(scratch))
        || under(folder, &std::env::temp_dir())
        // Windows' own, beside the person's
        || std::env::var_os("SystemRoot")
            .is_some_and(|root| under(folder, &std::path::Path::new(&root).join("Temp")))
}

/// The entries of `dir` that `wanted` keeps, each with when it last
/// changed, the newest first, and at most [`NEWEST`] of them.
fn newest(dir: &Path, wanted: &dyn Fn(&Path) -> bool) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| wanted(path))
        .filter_map(|path| {
            let at = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some((path, at))
        })
        .collect();
    found.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    found.truncate(NEWEST);
    found
}

fn is_session_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "jsonl") && path.is_file()
}

/// The folder a session file names: the `cwd` of the first of its lines
/// that has one, at the top or in its `payload`.
fn folder_in(session: &Path) -> Option<String> {
    let file = std::fs::File::open(session).ok()?;
    BufReader::new(file.take(HEAD))
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find_map(|line| {
            [&line["cwd"], &line["payload"]["cwd"]]
                .into_iter()
                .find_map(|cwd| cwd.as_str().map(str::to_string))
        })
}

/// Claude Code and Pi: a folder for each project under `root`, named after
/// the project's path with its separators lost, which holds a file for
/// each session. The newest session of each of the newest projects says
/// where it ran.
fn session_folders(root: &Path) -> Vec<(String, SystemTime)> {
    newest(root, &|path| path.is_dir())
        .into_iter()
        .filter_map(|(project, _)| {
            let (session, at) = newest(&project, &is_session_file).into_iter().next()?;
            Some((folder_in(&session)?, at))
        })
        .collect()
}

/// Codex: a session file in a folder for its year, month and day under
/// `root`. The newest days are read until enough sessions are found.
fn codex(root: &Path) -> Vec<(String, SystemTime)> {
    // the names sort as the dates do
    let latest = |dir: &Path| -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        dirs.reverse();
        dirs
    };
    let mut found = Vec::new();
    for year in latest(root) {
        for month in latest(&year) {
            for day in latest(&month) {
                for (session, at) in newest(&day, &is_session_file) {
                    if let Some(folder) = folder_in(&session) {
                        found.push((folder, at));
                    }
                    if found.len() >= NEWEST {
                        return found;
                    }
                }
            }
        }
    }
    found
}

/// Grok: a folder for each project under `root`, named after the project's
/// path, percent-encoded.
fn grok(root: &Path) -> Vec<(String, SystemTime)> {
    newest(root, &|path| path.is_dir())
        .into_iter()
        .filter_map(|(project, at)| {
            let name = project.file_name()?.to_str()?;
            Some((percent_decoded(name)?, at))
        })
        .collect()
}

/// `text` with each `%XX` as the byte it stands for; none when that is
/// not text.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// OpenCode: its projects, each with the folder it works in and when it
/// last did. They are in its database, and in a file each before that.
fn opencode(data: &Path) -> Vec<(String, SystemTime)> {
    let at = |millis: i64| {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(millis.max(0) as u64)
    };
    let from_database = || -> rusqlite::Result<Vec<(String, SystemTime)>> {
        // read only, and never waiting on OpenCode's own writes
        let db = rusqlite::Connection::open_with_flags(
            data.join("opencode.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut projects = db.prepare(
            "SELECT worktree, time_updated FROM project ORDER BY time_updated DESC LIMIT ?1",
        )?;
        projects
            .query_map([NEWEST as i64], |row| {
                Ok((row.get::<_, String>(0)?, at(row.get(1)?)))
            })?
            .collect()
    };
    if let Ok(found) = from_database() {
        return found;
    }
    newest(&data.join("storage/project"), &|path| {
        path.extension().is_some_and(|ext| ext == "json")
    })
    .into_iter()
    .filter_map(|(file, _)| {
        let project: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
        Some((
            project["worktree"].as_str()?.to_string(),
            at(project["time"]["updated"].as_i64()?),
        ))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder that is not there yet, under one named in its short form
    /// (ALICE~1, as a temporary folder can come), is the same folder as
    /// its long form; and a long path to a share is the share's path.
    #[cfg(windows)]
    #[test]
    fn a_folder_not_there_yet_compares_by_its_long_form() {
        let root = tempfile::tempdir().unwrap();
        let long = root.path().join("a folder of a long name");
        std::fs::create_dir(&long).unwrap();
        // cmd gives the short form, when the volume makes them
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("cmd")
            .arg("/c")
            .raw_arg(format!("for %I in (\"{}\") do @echo %~sI", long.display()))
            .output()
            .unwrap();
        let short = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if short.file_name() == long.file_name() {
            eprintln!("skipped: the volume makes no short names");
        } else {
            assert!(same_folder(&short.join("work"), &long.join("work")));
            assert!(under(&short.join("work"), &long));
            assert!(under(&long.join("work").join("deeper"), &short));
        }
        assert!(same_folder(
            Path::new(r"\\?\UNC\server\share\dir"),
            Path::new(r"\\server\share\dir")
        ));
        assert!(same_folder(
            Path::new(r"\\?\C:\Code"),
            Path::new(r"C:\Code")
        ));
    }

    /// Windows takes a path in any capitals, with either separator, and
    /// its temporary folder can come in its short form, ALICE~1.
    #[cfg(windows)]
    #[test]
    fn a_windows_folder_is_compared_as_windows_names_it() {
        let temp = std::env::temp_dir();
        let shouting = PathBuf::from(temp.display().to_string().to_uppercase()).join("work");
        assert!(is_scratch(&shouting), "{}", shouting.display());
        assert!(same_folder(
            Path::new(r"C:\Code\API"),
            Path::new("c:/code/api/")
        ));
        assert!(under(
            Path::new(r"C:\USERS\me\code"),
            Path::new(r"c:\users\me")
        ));
        assert!(!under(
            Path::new(r"C:\Users\meant"),
            Path::new(r"C:\Users\me")
        ));
    }
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    /// A home folder with project folders in it, and the time "now" in it.
    struct Home {
        dir: tempfile::TempDir,
        now: SystemTime,
    }

    impl Home {
        fn new() -> Home {
            Home {
                dir: tempfile::tempdir().unwrap(),
                now: SystemTime::now(),
            }
        }
        fn path(&self) -> &Path {
            self.dir.path()
        }
        /// A project folder that exists, by its name.
        fn project(&self, name: &str) -> String {
            let folder = self.path().join("code").join(name);
            std::fs::create_dir_all(&folder).unwrap();
            folder.display().to_string()
        }
        /// Writes a file under the home folder, last changed `minutes` ago.
        fn file(&self, path: &str, text: &str, minutes: u64) -> PathBuf {
            let file = self.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, text).unwrap();
            self.aged(&file, minutes);
            file
        }
        fn aged(&self, path: &Path, minutes: u64) {
            let at = self.now - Duration::from_secs(minutes * 60);
            let mut options = std::fs::File::options();
            options.read(true);
            // Windows changes the time through a handle that may write the
            // attributes, and opens a folder only with backup semantics
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
                const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
                options
                    .access_mode(FILE_WRITE_ATTRIBUTES)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
            }
            options.open(path).unwrap().set_modified(at).unwrap();
        }
        fn recent(&self, limit: usize) -> Vec<(String, &'static str)> {
            recent_where(self.path(), limit, &|_| true)
                .into_iter()
                .map(|used| (used.folder, used.agent))
                .collect()
        }
    }

    /// A path as it stands inside a JSON string, with Windows's
    /// backslashes escaped.
    fn json_text(path: &str) -> String {
        let quoted = serde_json::to_string(path).unwrap();
        quoted[1..quoted.len() - 1].to_string()
    }

    #[test]
    fn claude_code_names_the_folder_in_a_line_of_its_newest_session() {
        let home = Home::new();
        let api = home.project("my-api");
        let web = home.project("web");
        // the folder's name loses what the path had: my-api or my/api
        // the first lines of a session name no folder
        let session = |cwd: &str| {
            let cwd = json_text(cwd);
            format!(
                "{{\"type\":\"mode\",\"sessionId\":\"s1\"}}\n\
                 {{\"type\":\"permissionMode\",\"sessionId\":\"s1\"}}\n\
                 {{\"type\":\"user\",\"cwd\":\"{cwd}\",\"sessionId\":\"s1\",\"message\":{{\"content\":\"hello\"}}}}\n"
            )
        };
        home.file(
            ".claude/projects/-code-my-api/old.jsonl",
            &session("/gone"),
            90,
        );
        home.file(
            ".claude/projects/-code-my-api/new.jsonl",
            &session(&api),
            10,
        );
        home.file(".claude/projects/-code-web/one.jsonl", &session(&web), 30);
        // not a session
        home.file(".claude/projects/-code-web/notes.txt", "cwd", 1);

        assert_eq!(home.recent(8), [(api, "claude-code"), (web, "claude-code")]);
    }

    #[test]
    fn codex_names_the_folder_in_the_first_line_of_a_session() {
        let home = Home::new();
        let api = home.project("api");
        let web = home.project("web");
        let meta = |cwd: &str| {
            let cwd = json_text(cwd);
            format!(
                "{{\"timestamp\":\"2026-11-20T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"x\",\"cwd\":\"{cwd}\",\"cli_version\":\"0.98.0\"}}}}\n{{\"type\":\"event\"}}\n"
            )
        };
        home.file(
            ".codex/sessions/2026/11/20/rollout-2026-11-20T10-00-00-a.jsonl",
            &meta(&api),
            20,
        );
        home.file(
            ".codex/sessions/2026/11/19/rollout-2026-11-19T09-00-00-b.jsonl",
            &meta(&web),
            60,
        );
        home.file(
            ".codex/sessions/2025/12/31/rollout-2025-12-31T09-00-00-c.jsonl",
            &meta(&api),
            900,
        );

        assert_eq!(home.recent(8), [(api, "codex"), (web, "codex")]);
    }

    #[test]
    fn pi_names_the_folder_in_the_first_line_of_a_session() {
        let home = Home::new();
        let api = home.project("api");
        let line = format!(
            "{{\"type\":\"session\",\"version\":3,\"id\":\"x\",\"timestamp\":\"2026-11-20T15:16:35Z\",\"cwd\":\"{}\"}}\n",
            json_text(&api)
        );
        home.file(
            ".pi/agent/sessions/--code-api--/2026-11-20T15-16-35-039Z_x.jsonl",
            &line,
            15,
        );
        assert_eq!(home.recent(8), [(api, "pi")]);
    }

    #[test]
    fn grok_names_the_folder_in_the_name_of_its_sessions_folder() {
        let home = Home::new();
        let api = home.project("my api");
        // what a folder's name cannot hold, on either system
        let encoded = api
            .replace('/', "%2F")
            .replace('\\', "%5C")
            .replace(':', "%3A")
            .replace(' ', "%20");
        let dir = home.path().join(".grok/sessions").join(&encoded);
        std::fs::create_dir_all(dir.join("0193-session")).unwrap();
        home.aged(&dir, 5);
        // its search index is no session
        home.file(".grok/sessions/session_search.sqlite", "", 1);

        assert_eq!(home.recent(8), [(api, "grok")]);
    }

    #[test]
    fn opencode_names_the_folders_in_its_database() {
        let home = Home::new();
        let api = home.project("api");
        let web = home.project("web");
        let data = home.path().join(".local/share/opencode");
        std::fs::create_dir_all(&data).unwrap();
        let db = rusqlite::Connection::open(data.join("opencode.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE project (id text PRIMARY KEY, worktree text NOT NULL, time_updated integer NOT NULL);",
        )
        .unwrap();
        let ago = |minutes: i64| Utc::now().timestamp_millis() - minutes * 60_000;
        for (id, worktree, at) in [
            ("a", api.as_str(), ago(40)),
            ("b", web.as_str(), ago(5)),
            ("g", "/", ago(1)),
        ] {
            db.execute(
                "INSERT INTO project VALUES (?1, ?2, ?3)",
                rusqlite::params![id, worktree, at],
            )
            .unwrap();
        }
        drop(db);

        assert_eq!(home.recent(8), [(web, "opencode"), (api, "opencode")]);
    }

    #[test]
    fn an_older_opencode_names_the_folders_in_its_project_files() {
        let home = Home::new();
        let api = home.project("api");
        let at = Utc::now().timestamp_millis() - 60_000;
        home.file(
            ".local/share/opencode/storage/project/abc.json",
            &format!("{{\"id\":\"abc\",\"worktree\":\"{}\",\"vcs\":\"git\",\"time\":{{\"created\":1,\"updated\":{at}}}}}", json_text(&api)),
            1,
        );
        assert_eq!(home.recent(8), [(api, "opencode")]);
    }

    #[test]
    fn the_folders_of_every_agent_come_newest_first_without_repeats_or_missing_ones() {
        let home = Home::new();
        let api = home.project("api");
        let web = home.project("web");
        let cwd =
            |folder: &str| format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", json_text(folder));
        home.file(".claude/projects/-api/a.jsonl", &cwd(&api), 50);
        home.file(".claude/projects/-web/a.jsonl", &cwd(&web), 30);
        home.file(".claude/projects/-gone/a.jsonl", &cwd("/no/such/folder"), 1);
        // Pi worked in api after Claude Code did
        home.file(".pi/agent/sessions/--api--/2026_x.jsonl", &cwd(&api), 10);

        assert_eq!(home.recent(8), [(api.clone(), "pi"), (web, "claude-code")]);
        assert_eq!(home.recent(1), [(api, "pi")]);
        // a computer with no agent has no history
        assert_eq!(
            recent_where(tempfile::tempdir().unwrap().path(), 8, &|_| true),
            []
        );
    }

    #[test]
    fn scratch_folders_are_no_project_s() {
        for folder in [
            "/tmp/work",
            "/private/tmp/x/y",
            "/var/folders/6k/T/pinrail",
            "/private/var/folders/a",
        ] {
            assert!(is_scratch(Path::new(folder)), "{folder}");
        }
        for folder in ["/Users/me/code/api", "/home/me/tmp-project", "/srv/app"] {
            assert!(!is_scratch(Path::new(folder)), "{folder}");
        }
    }
}
