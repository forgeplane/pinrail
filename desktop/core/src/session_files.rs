//! The files the person attaches to a session's prompts, such as a pasted
//! screenshot. Each is saved under `sessions/<agent>/<session>/files/`
//! with its own name, so that the agent can open it by its path, and the
//! folder goes when the session does.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::error::Error;

/// The most one file may be: 100 MB.
pub const MAX_BYTES: u64 = 100 * 1024 * 1024;

/// The longest name a saved file keeps.
const NAME_MAX: usize = 120;

#[derive(Debug)]
pub struct SessionFiles {
    root: PathBuf,
    db: Arc<Db>,
}

/// A file being written: its path, and the bytes so far.
pub struct Saving {
    path: PathBuf,
    file: Option<File>,
    written: u64,
}

impl SessionFiles {
    /// The files under `root`, one folder per agent and session.
    pub fn new(root: PathBuf, db: Arc<Db>) -> Self {
        Self { root, db }
    }

    /// The folder of a session's files, which may not exist yet. `None`
    /// when the agent's or the session's name could not be a folder's.
    pub fn folder(&self, agent: &str, id: &str) -> Option<PathBuf> {
        (segment(agent) && segment(id)).then(|| self.root.join(agent).join(id).join("files"))
    }

    /// Starts saving a file named after `name` for a session the core
    /// knows, under a name no other of its files has.
    pub fn begin(&self, agent: &str, id: &str, name: &str) -> Result<Saving, Error> {
        let missing = || Error::NotFound(format!("session {id} of {agent}"));
        let folder = self.folder(agent, id).ok_or_else(missing)?;
        if !self.db.has_session(agent, id)? {
            return Err(missing());
        }
        fs::create_dir_all(&folder)?;
        let name = file_name(name);
        let (stem, ext) = split(&name);
        for n in 1.. {
            let candidate = if n == 1 {
                name.clone()
            } else {
                let suffix = format!("-{n}{ext}");
                format!("{}{suffix}", cut(stem, NAME_MAX - suffix.len()))
            };
            let path = folder.join(&candidate);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Saving {
                        path,
                        file: Some(file),
                        written: 0,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!("some name is free")
    }

    /// A saved file of the session, by the name it was saved under; `None`
    /// for any other name, such as one with a folder in it.
    pub fn file(&self, agent: &str, id: &str, name: &str) -> Option<PathBuf> {
        // a name the cleaning would change is not one a file was saved under
        if file_name(name) != name {
            return None;
        }
        let path = self.folder(agent, id)?.join(name);
        path.is_file().then_some(path)
    }

    /// Deletes a session's files.
    pub fn remove(&self, agent: &str, id: &str) {
        if let Some(folder) = self.folder(agent, id) {
            let _ = fs::remove_dir_all(folder.parent().unwrap_or(&folder));
        }
    }

    /// Deletes the files of the sessions the core no longer knows, however
    /// they went. Returns how many sessions' files it deleted.
    pub fn sweep(&self) -> Result<usize, Error> {
        let mut deleted = 0;
        let Ok(agents) = fs::read_dir(&self.root) else {
            return Ok(0);
        };
        for agent in agents.flatten() {
            let Ok(sessions) = fs::read_dir(agent.path()) else {
                continue;
            };
            let agent_name = agent.file_name().to_string_lossy().into_owned();
            for session in sessions.flatten() {
                let id = session.file_name().to_string_lossy().into_owned();
                if !self.db.has_session(&agent_name, &id)? {
                    let _ = fs::remove_dir_all(session.path());
                    deleted += 1;
                }
            }
            // an agent's folder with no session left
            let _ = fs::remove_dir(agent.path());
        }
        Ok(deleted)
    }
}

impl Saving {
    /// Adds bytes. `false` once the file passes [`MAX_BYTES`]: it is then
    /// not kept.
    pub fn write(&mut self, bytes: &[u8]) -> Result<bool, Error> {
        self.written += bytes.len() as u64;
        if self.written > MAX_BYTES {
            return Ok(false);
        }
        let file = self.file.as_mut().expect("written before it is finished");
        file.write_all(bytes)?;
        Ok(true)
    }

    /// The saved file's path.
    pub fn finish(mut self) -> Result<PathBuf, Error> {
        let file = self.file.take().expect("finished once");
        file.sync_all()?;
        Ok(self.path.clone())
    }
}

impl Drop for Saving {
    /// A file not finished, whatever stopped it, is not kept.
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Whether a name can be one folder: not empty, not hidden, and no
/// separators or other characters a path treats specially.
fn segment(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// A name a file can have, from the one it came with: no folders, no
/// spaces or brackets, not hidden, at most [`NAME_MAX`] bytes, as the
/// window names attachments (`lib/attach.ts`). And one every system can
/// hold: none of what Windows refuses (`: " | ? *`), no dot at the end,
/// which Windows drops, and not a device's name, such as `CON`.
fn file_name(name: &str) -> String {
    // only the last part of a path
    let name = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut cleaned = String::new();
    let mut dash = false;
    for c in name.chars().filter(|c| !c.is_control()) {
        if c.is_whitespace()
            || matches!(
                c,
                '(' | ')' | '[' | ']' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
            )
        {
            if !dash {
                cleaned.push('-');
                dash = true;
            }
        } else {
            cleaned.push(c);
            dash = false;
        }
    }
    let cleaned = cleaned.trim_start_matches(['.', '-']).trim_end_matches('.');
    let (stem, ext) = split(cleaned);
    // a device's name is told from what comes before the first dot, which
    // the cut leaves as it is; room is kept for what goes before it
    let prefix = if device(stem) { "file-" } else { "" };
    let mut stem = cut(stem, NAME_MAX.saturating_sub(prefix.len() + ext.len()));
    if ext.is_empty() {
        // the cut may end at a dot, which would be the name's last
        stem = stem.trim_end_matches('.');
    }
    if stem.is_empty() {
        format!("file{ext}")
    } else {
        format!("{prefix}{stem}{ext}")
    }
}

/// Whether a name's stem is a device's on Windows, with any extension:
/// `con`, `nul` and the like, and `com` or `lpt` with one digit, which may
/// be one of the superscripts ¹ ² ³.
fn device(stem: &str) -> bool {
    let base = stem.split('.').next().unwrap_or(stem).to_ascii_lowercase();
    if matches!(
        base.as_str(),
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
    ) {
        return true;
    }
    let Some(number) = base
        .strip_prefix("com")
        .or_else(|| base.strip_prefix("lpt"))
    else {
        return false;
    };
    let mut digits = number.chars();
    matches!(
        (digits.next(), digits.next()),
        (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

/// A name's stem and extension. What follows the last dot is no extension
/// when it is longer than 20 characters.
fn split(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= 21 => name.split_at(dot),
        _ => (name, ""),
    }
}

/// The longest start of `text` of at most `max` bytes that ends on a
/// character's boundary.
fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The folder of every session's files, under the data directory.
pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_windows_cannot_hold_is_made_one_it_can() {
        // a stream on NTFS, and what Windows refuses in a name
        assert_eq!(
            file_name("Screenshot 12:30:00.png"),
            "Screenshot-12-30-00.png"
        );
        assert_eq!(file_name("notes.txt:ads"), "notes.txt-ads");
        assert_eq!(file_name("a|b?c*d\"e.txt"), "a-b-c-d-e.txt");
        // a trailing dot Windows would drop, and a device's name
        assert_eq!(file_name("report."), "report");
        assert_eq!(file_name("CON.txt"), "file-CON.txt");
        assert_eq!(file_name("nul"), "file-nul");
        assert_eq!(file_name("com1.log"), "file-com1.log");
        assert_eq!(file_name("console.txt"), "console.txt");
    }

    #[test]
    fn a_name_loses_its_folders_spaces_and_leading_dots() {
        assert_eq!(file_name("../a b/c (1).png"), "c-1-.png");
        assert_eq!(file_name(".env"), "env");
        assert_eq!(file_name("Screen Shot  2026.png"), "Screen-Shot-2026.png");
        assert_eq!(file_name("..."), "file");
        assert_eq!(file_name(&format!("{}.png", "é".repeat(100))).len(), 120);
    }

    /// A cleaned name is one the cleaning leaves alone and that fits, which
    /// is what [`SessionFiles::file`] asks of a saved file's name.
    fn holds(name: &str) -> String {
        let out = file_name(name);
        assert_eq!(file_name(&out), out, "cleaning {name:?} again");
        assert!(out.len() <= NAME_MAX, "{out:?} is {} bytes", out.len());
        assert!(!out.ends_with('.'), "{out:?} ends with a dot");
        out
    }

    #[test]
    fn a_long_name_of_a_device_still_fits() {
        // an extension too long to be one, so the whole name is the stem
        let out = holds(&format!("CON.{}", "a".repeat(120)));
        assert!(out.starts_with("file-CON."));
        let out = holds(&format!("{}.txt", "nul.".repeat(40)));
        assert!(out.starts_with("file-nul.") && out.ends_with(".txt"));
        assert_eq!(holds(&format!("aux.{}", "b".repeat(111))).len(), 120);
    }

    #[test]
    fn a_name_cut_at_a_dot_does_not_end_with_it() {
        let out = holds(&format!("{}.{}", "a".repeat(119), "b".repeat(25)));
        assert_eq!(out, "a".repeat(119));
        let out = holds(&format!("{}...{}", "a".repeat(118), "b".repeat(25)));
        assert_eq!(out, "a".repeat(118));
        // with an extension the dot is inside the name, and stays
        holds(&format!("{}.{}.png", "a".repeat(115), "b".repeat(25)));
    }

    #[test]
    fn a_device_numbered_in_superscript_is_one_too() {
        for name in ["com\u{b9}", "COM\u{b2}.txt", "lpt\u{b3}.log", "Lpt\u{b9}"] {
            assert_eq!(holds(name), format!("file-{name}"));
        }
        assert_eq!(holds("com\u{2074}.txt"), "com\u{2074}.txt");
        assert_eq!(holds("com12.txt"), "com12.txt");
        assert_eq!(holds("lpt.txt"), "lpt.txt");
        assert_eq!(holds("lpt0"), "file-lpt0");
    }

    #[test]
    fn a_session_folder_is_one_plain_segment() {
        assert!(segment("claude-code"));
        assert!(segment("5f0c1a2b-3c4d"));
        assert!(!segment(".."));
        assert!(!segment("a/b"));
        assert!(!segment(""));
    }
}
