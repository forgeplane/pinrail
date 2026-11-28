//! The git checkout a folder is in: its top, the project its remote names,
//! and the branch. The CLI fills an origin from it, and the core a session
//! that Pinrail starts.

use std::path::Path;
use std::process::{Command, Stdio};

/// What a folder's checkout says of where work happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    /// The top of the checkout.
    pub top: String,
    /// The project its remote names, such as `acme/api`, or the top's last
    /// folder without a remote.
    pub repo: Option<String>,
    /// The branch, even before its first commit; none on a detached HEAD.
    pub reference: Option<String>,
}

/// The checkout `folder` is in, or `None` outside one, or without git.
pub fn of(folder: &Path) -> Option<Checkout> {
    let git = |args: &[&str]| git(folder, args);
    let top = git(&["rev-parse", "--show-toplevel"])?;
    let remote = git(&["remote", "get-url", "origin"]).or_else(|| {
        let first = git(&["remote"])?.lines().next()?.to_string();
        git(&["remote", "get-url", &first])
    });
    let repo = remote
        .as_deref()
        .and_then(project_of)
        .or_else(|| top.rsplit('/').next().map(str::to_string))
        .filter(|r| !r.is_empty());
    let reference = git(&["symbolic-ref", "--short", "HEAD"]);
    Some(Checkout {
        top,
        repo,
        reference,
    })
}

fn git(folder: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    // the app reads checkouts, and has no console for git to show
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(
        &mut command,
        windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
    );
    let out = command
        .arg("-C")
        .arg(folder)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// The project a remote names: its path on the host, without `.git`.
/// `git@github.com:acme/api.git` and `https://github.com/acme/api` are
/// `acme/api`; a subgroup stays in (`group/sub/api`); a local path gives its
/// last folder.
pub fn project_of(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches(['/', '\\']);
    let url = url.strip_suffix(".git").unwrap_or(url);
    let path = if let Some((_, rest)) = url.split_once("://") {
        // scheme://[user@]host[:port]/path
        rest.split_once('/').map(|(_, path)| path)?
    } else if url.starts_with('/')
        || url.starts_with('.')
        || url.starts_with('~')
        || windows_path(url)
    {
        url.rsplit(['/', '\\']).next()?
    } else if let Some((_, path)) = url.split_once(':') {
        // [user@]host:path
        path
    } else {
        url
    };
    let path = path.trim_matches('/');
    (!path.is_empty()).then(|| path.to_string())
}

/// Whether `url` is a Windows path, `C:\…`, `C:/…` or `\\server\…`,
/// which git takes for a local remote, not for `host:path`.
fn windows_path(url: &str) -> bool {
    let bytes = url.as_bytes();
    url.starts_with("\\\\")
        || bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_windows_folder_ending_in_a_separator_names_its_project() {
        assert_eq!(project_of(r"C:\code\api\").as_deref(), Some("api"));
        assert_eq!(project_of(r"C:\code\api").as_deref(), Some("api"));
    }
    use super::project_of;

    #[test]
    fn a_remote_names_its_project_by_its_path() {
        for (url, project) in [
            ("git@github.com:acme/api.git", "acme/api"),
            ("https://github.com/acme/api.git", "acme/api"),
            ("https://github.com/acme/api", "acme/api"),
            ("https://github.com/acme/api/", "acme/api"),
            (
                "ssh://git@gitlab.example.com:2222/group/sub/api.git",
                "group/sub/api",
            ),
            ("https://token@gitlab.com/group/api.git", "group/api"),
            ("/srv/git/api.git", "api"),
            ("../api", "api"),
            (r"C:\srv\git\api.git", "api"),
            ("C:/srv/git/api.git", "api"),
            (r"\\server\git\api.git", "api"),
        ] {
            assert_eq!(project_of(url).as_deref(), Some(project), "{url}");
        }
        assert_eq!(project_of("https://github.com"), None);
    }
}
