//! The folders agents work in, as the Start an agent dialog offers them:
//! the recent ones, with their repository and the branch checked out now,
//! and the ones the agents worked in before Pinrail saw them.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::db::Db;

/// How many recent folders the dialog offers.
pub const RECENT: usize = 20;
/// How many more it offers from the agents' own history.
pub const SUGGESTED: usize = 8;

/// The recent folders, most recently used first: each with the repository
/// its sessions named, the branch checked out in it now, and whether it
/// still exists. After the ones Pinrail has seen come the ones the agents
/// under `history`, the person's home folder, worked in without it, which
/// are still there; each of those names its agent.
pub fn recent(db: &Db, history: Option<&Path>) -> rusqlite::Result<Vec<Value>> {
    let mut folders: Vec<Value> = db
        .recent_folders(RECENT)?
        .into_iter()
        .map(|(folder, repo, used_at)| {
            let path = Path::new(&folder);
            let exists = path.is_dir();
            // a folder outside any checkout is no repository's, whatever a
            // session reported from it; one that is gone keeps its record
            let repo = if exists && git_dir(path).is_none() {
                None
            } else {
                repo
            };
            json!({
                "folder": folder,
                "repo": repo,
                "branch": if exists { branch(path) } else { None },
                "exists": exists,
                "used_at": used_at,
                "agent": null,
            })
        })
        .collect();
    let suggested = history
        .map(|home| crate::agent_history::recent_folders(home, SUGGESTED))
        .unwrap_or_default();
    for used in suggested {
        if folders.iter().any(|seen| seen["folder"] == used.folder) {
            continue;
        }
        folders.push(json!({
            "folder": used.folder,
            "repo": null,
            "branch": branch(Path::new(&used.folder)),
            "exists": true,
            "used_at": crate::reviews::iso(used.used_at),
            "agent": used.agent,
        }));
    }
    // every time is written the same way, so they sort as text
    folders.sort_by(|a, b| b["used_at"].as_str().cmp(&a["used_at"].as_str()));
    Ok(folders)
}

/// The branch checked out in the Git repository or worktree that holds
/// `folder`, read from its `HEAD`; `None` outside a repository and on a
/// detached `HEAD`.
pub fn branch(folder: &Path) -> Option<String> {
    let git_dir = git_dir(folder)?;
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_string)
}

/// Whether `folder` is in a Git repository or worktree.
pub fn in_checkout(folder: &str) -> bool {
    git_dir(Path::new(folder)).is_some()
}

/// The Git folder of the repository holding `folder`: `.git` itself, or
/// the folder a worktree's `.git` file points to.
fn git_dir(folder: &Path) -> Option<PathBuf> {
    folder.ancestors().find_map(|dir| {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        Some(dir.join(target))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_branch_is_read_from_the_repository_s_head() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(".git/HEAD"), "ref: refs/heads/fix-login\n");
        assert_eq!(branch(dir.path()).as_deref(), Some("fix-login"));
        // and from a folder inside it
        let inner = dir.path().join("src/app");
        std::fs::create_dir_all(&inner).unwrap();
        assert_eq!(branch(&inner).as_deref(), Some("fix-login"));
    }

    #[test]
    fn a_worktree_s_branch_is_read_from_the_folder_its_git_file_names() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("app");
        write(
            &main.join(".git/worktrees/feature/HEAD"),
            "ref: refs/heads/feature\n",
        );
        let worktree = dir.path().join("app-feature");
        write(
            &worktree.join(".git"),
            &format!(
                "gitdir: {}\n",
                main.join(".git/worktrees/feature").display()
            ),
        );
        assert_eq!(branch(&worktree).as_deref(), Some("feature"));
    }

    #[test]
    fn a_detached_head_or_a_plain_folder_has_no_branch() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(branch(dir.path()), None);
        write(&dir.path().join(".git/HEAD"), "1f2e3d4c5b6a\n");
        assert_eq!(branch(dir.path()), None);
    }
}
