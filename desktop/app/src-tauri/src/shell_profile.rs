//! Putting `~/.local/bin` on the PATH of the person's shell, by adding one
//! line to the file the shell reads when a terminal starts. Only the shells
//! Pinrail knows the file and the syntax of get the line: zsh, bash and
//! fish. On Windows the PATH is in the registry, which `cli_install` changes.

use std::path::{Path, PathBuf};

/// The comment above the line, which says where it came from.
#[cfg_attr(windows, allow(dead_code))]
const MARK: &str = "# Added by Pinrail";
/// The line for zsh and bash.
const POSIX_LINE: &str = r#"export PATH="$HOME/.local/bin:$PATH""#;
/// The line for fish.
const FISH_LINE: &str = r#"fish_add_path "$HOME/.local/bin""#;

/// The file a shell reads at start, and the line that puts `~/.local/bin` on
/// its PATH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub file: PathBuf,
    pub line: &'static str,
}

/// The profile of `shell`, the path of the person's login shell, or none
/// for a shell Pinrail does not know. `macos` says which file bash reads in
/// a new terminal, and `var` reads an environment variable.
#[cfg_attr(windows, allow(dead_code))]
pub fn profile(
    shell: &str,
    home: &Path,
    macos: bool,
    var: &dyn Fn(&str) -> Option<String>,
) -> Option<Profile> {
    let folder = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let name = Path::new(shell).file_name()?.to_str()?;
    let (file, line) = match name {
        "zsh" => (
            folder("ZDOTDIR")
                .unwrap_or_else(|| home.to_path_buf())
                .join(".zshrc"),
            POSIX_LINE,
        ),
        // a terminal on macOS starts bash as a login shell, which reads the
        // first of these that is there, and never .bashrc on its own
        "bash" if macos => (
            [".bash_profile", ".bash_login", ".profile"]
                .iter()
                .map(|name| home.join(name))
                .find(|file| file.is_file())
                .unwrap_or_else(|| home.join(".bash_profile")),
            POSIX_LINE,
        ),
        "bash" => (home.join(".bashrc"), POSIX_LINE),
        "fish" => (
            folder("XDG_CONFIG_HOME")
                .unwrap_or_else(|| home.join(".config"))
                .join("fish")
                .join("config.fish"),
            FISH_LINE,
        ),
        _ => return None,
    };
    Some(Profile { file, line })
}

/// Adds the profile's line at the end of its file, under a comment, and
/// creates the file when it is missing. The rest of the file and its
/// permissions stay as they are. False when the line was already there.
#[cfg_attr(windows, allow(dead_code))]
pub fn add(profile: &Profile) -> Result<bool, String> {
    // a link, as a dotfiles folder makes, stays one: its file gets the line
    let file = std::fs::canonicalize(&profile.file).unwrap_or_else(|_| profile.file.clone());
    let fail = |what: &str, e: std::io::Error| format!("cannot {what} {}: {e}", file.display());
    let before = match std::fs::read_to_string(&file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(fail("read", e)),
    };
    let text = before.as_deref().unwrap_or_default();
    if text.lines().any(|line| line.trim() == profile.line) {
        return Ok(false);
    }
    let mut after = text.to_string();
    if !after.is_empty() {
        if !after.ends_with('\n') {
            after.push('\n');
        }
        after.push('\n');
    }
    after.push_str(&format!("{MARK}\n{}\n", profile.line));

    let dir = file.parent().ok_or("the profile has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    // beside the file, then renamed over it, so a shell never reads half of it
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let partial = file.with_file_name(format!("{name}.pinrail-partial"));
    let result = std::fs::write(&partial, after)
        .and_then(|()| match std::fs::metadata(&file) {
            Ok(meta) => std::fs::set_permissions(&partial, meta.permissions()),
            Err(_) => Ok(()),
        })
        .and_then(|()| std::fs::rename(&partial, &file));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result.map(|()| true).map_err(|e| fail("write", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_var(_: &str) -> Option<String> {
        None
    }

    fn of(shell: &str, home: &Path, macos: bool) -> Option<Profile> {
        profile(shell, home, macos, &no_var)
    }

    #[test]
    fn zsh_reads_zshrc_in_the_home_folder_or_in_zdotdir() {
        let home = tempfile::tempdir().unwrap();
        let found = of("/bin/zsh", home.path(), true).unwrap();
        assert_eq!(found.file, home.path().join(".zshrc"));
        assert_eq!(found.line, r#"export PATH="$HOME/.local/bin:$PATH""#);

        let elsewhere = |name: &str| (name == "ZDOTDIR").then(|| "/somewhere/zsh".to_string());
        let found = profile("/opt/homebrew/bin/zsh", home.path(), true, &elsewhere).unwrap();
        assert_eq!(found.file, Path::new("/somewhere/zsh/.zshrc"));
    }

    #[test]
    fn bash_reads_bashrc_on_linux_and_its_login_file_on_macos() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            of("/usr/bin/bash", home.path(), false).unwrap().file,
            home.path().join(".bashrc")
        );
        // a terminal on macOS starts a login shell: with none of its files
        // there, .bash_profile is the one to create
        assert_eq!(
            of("/bin/bash", home.path(), true).unwrap().file,
            home.path().join(".bash_profile")
        );
        // bash reads only the first of its login files that is there
        std::fs::write(home.path().join(".profile"), "").unwrap();
        assert_eq!(
            of("/bin/bash", home.path(), true).unwrap().file,
            home.path().join(".profile")
        );
        std::fs::write(home.path().join(".bash_profile"), "").unwrap();
        assert_eq!(
            of("/bin/bash", home.path(), true).unwrap().file,
            home.path().join(".bash_profile")
        );
    }

    #[test]
    fn fish_reads_its_config_file_and_has_its_own_line() {
        let home = tempfile::tempdir().unwrap();
        let found = of("/usr/local/bin/fish", home.path(), true).unwrap();
        assert_eq!(found.file, home.path().join(".config/fish/config.fish"));
        assert_eq!(found.line, r#"fish_add_path "$HOME/.local/bin""#);

        let xdg = |name: &str| (name == "XDG_CONFIG_HOME").then(|| "/cfg".to_string());
        let found = profile("fish", home.path(), false, &xdg).unwrap();
        assert_eq!(found.file, Path::new("/cfg/fish/config.fish"));
    }

    #[test]
    fn a_shell_pinrail_does_not_know_has_no_profile() {
        let home = tempfile::tempdir().unwrap();
        for shell in ["/bin/sh", "/bin/csh", "/usr/bin/nu", "/bin/tcsh", ""] {
            assert_eq!(of(shell, home.path(), true), None, "{shell}");
        }
    }

    #[test]
    fn a_missing_file_is_created_with_its_folders() {
        let home = tempfile::tempdir().unwrap();
        let fish = of("fish", home.path(), false).unwrap();
        assert_eq!(add(&fish), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&fish.file).unwrap(),
            "# Added by Pinrail\nfish_add_path \"$HOME/.local/bin\"\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_line_goes_after_what_the_file_holds_and_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let zsh = of("zsh", home.path(), true).unwrap();
        // no newline at the end, as a file edited by hand may have
        std::fs::write(&zsh.file, "alias ll='ls -l'\nexport EDITOR=vim").unwrap();
        std::fs::set_permissions(&zsh.file, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert_eq!(add(&zsh), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&zsh.file).unwrap(),
            "alias ll='ls -l'\nexport EDITOR=vim\n\n# Added by Pinrail\nexport PATH=\"$HOME/.local/bin:$PATH\"\n"
        );
        let mode = std::fs::metadata(&zsh.file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        // nothing is left beside it
        let names: Vec<_> = std::fs::read_dir(home.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [".zshrc"]);
    }

    #[test]
    fn a_line_already_there_is_not_added_again() {
        let home = tempfile::tempdir().unwrap();
        let bash = of("bash", home.path(), false).unwrap();
        assert_eq!(add(&bash), Ok(true));
        let once = std::fs::read_to_string(&bash.file).unwrap();
        assert_eq!(add(&bash), Ok(false));
        assert_eq!(std::fs::read_to_string(&bash.file).unwrap(), once);

        // the person's own line counts, indented or not
        let zsh = of("zsh", home.path(), false).unwrap();
        let own = "if true; then\n  export PATH=\"$HOME/.local/bin:$PATH\"\nfi\n";
        std::fs::write(&zsh.file, own).unwrap();
        assert_eq!(add(&zsh), Ok(false));
        assert_eq!(std::fs::read_to_string(&zsh.file).unwrap(), own);
    }

    #[cfg(unix)]
    #[test]
    fn a_profile_that_is_a_link_stays_a_link() {
        // as a dotfiles folder has it: the file it points at gets the line
        let home = tempfile::tempdir().unwrap();
        let dotfiles = home.path().join("dotfiles");
        std::fs::create_dir(&dotfiles).unwrap();
        std::fs::write(dotfiles.join("zshrc"), "# mine\n").unwrap();
        let zsh = of("zsh", home.path(), true).unwrap();
        std::os::unix::fs::symlink(dotfiles.join("zshrc"), &zsh.file).unwrap();

        assert_eq!(add(&zsh), Ok(true));
        assert!(
            std::fs::symlink_metadata(&zsh.file)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(dotfiles.join("zshrc")).unwrap(),
            "# mine\n\n# Added by Pinrail\nexport PATH=\"$HOME/.local/bin:$PATH\"\n"
        );
    }
}
