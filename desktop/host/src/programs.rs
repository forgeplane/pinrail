//! Agents' programs as the agents would find them: on the `PATH` of the
//! environment agents start from, with the version each program reports.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::environment::find_program;

/// How long a program has to print its version.
const VERSION_WITHIN: Duration = Duration::from_secs(5);

/// Each of `names`, with its path and version, or `null` for both when it
/// is not on `env`'s `PATH`.
pub fn find(names: &[String], env: &[(String, String)]) -> Vec<Value> {
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.as_str());
    names
        .iter()
        .map(|name| {
            let found = find_program(name, path).filter(|p| p.is_file());
            let version = found.as_deref().and_then(|p| version(p, env));
            json!({
                "name": name,
                "path": found.map(|p| p.to_string_lossy().into_owned()),
                "version": version,
            })
        })
        .collect()
}

/// The version `program --version` prints: the first word that starts
/// with a digit, such as `2.1.325` in `2.1.325 (Claude Code)`.
fn version(program: &Path, env: &[(String, String)]) -> Option<String> {
    // in a folder of no one's, which a program that writes there harms not
    let mut child = Command::new(program)
        .arg("--version")
        .current_dir(std::env::temp_dir())
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < VERSION_WITHIN => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    version_in(&String::from_utf8_lossy(&output.stdout))
}

fn version_in(text: &str) -> Option<String> {
    text.lines().next()?.split_whitespace().find_map(|word| {
        let word = word.trim_start_matches('v');
        word.starts_with(|c: char| c.is_ascii_digit())
            .then(|| word.trim_end_matches([',', ')', '.']).to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Links `name` in `dir` to a script that runs `script`. The script is
    /// written once for its content, in a folder that outlives the test:
    /// macOS scans a new executable the first time it runs, which can take
    /// longer than a program has to print its version.
    fn program(dir: &Path, name: &str, script: &str) {
        use std::hash::{Hash, Hasher};
        let content = format!("#!/bin/sh\n{script}\n");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        let kept = std::env::temp_dir()
            .join("pinrail-test-programs")
            .join(format!("{:016x}", hasher.finish()));
        if !kept.is_file() {
            std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
            // written beside it and renamed, as another test may run it meanwhile
            let fresh = kept.with_extension(std::process::id().to_string());
            std::fs::write(&fresh, content).unwrap();
            std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::rename(&fresh, &kept).unwrap();
        }
        std::os::unix::fs::symlink(&kept, dir.join(name)).unwrap();
    }

    #[test]
    fn a_program_on_the_path_is_found_with_its_version() {
        let dir = tempfile::tempdir().unwrap();
        program(dir.path(), "claude", "echo '2.1.325 (Claude Code)'");
        program(dir.path(), "codex", "echo 'codex-cli 0.98.0'");
        let env = vec![(
            "PATH".to_string(),
            dir.path().to_string_lossy().into_owned(),
        )];
        let found = find(&["claude".into(), "codex".into()], &env);
        assert_eq!(found[0]["version"], "2.1.325");
        assert_eq!(found[1]["version"], "0.98.0");
        assert_eq!(
            found[0]["path"],
            dir.path().join("claude").to_string_lossy().as_ref()
        );
    }

    #[test]
    fn a_missing_program_has_no_path_and_no_version() {
        let dir = tempfile::tempdir().unwrap();
        let env = vec![(
            "PATH".to_string(),
            dir.path().to_string_lossy().into_owned(),
        )];
        let found = find(&["claude".into()], &env);
        assert_eq!(
            found[0],
            json!({"name": "claude", "path": null, "version": null})
        );
    }

    #[test]
    fn a_program_that_prints_no_version_is_still_found() {
        let dir = tempfile::tempdir().unwrap();
        program(dir.path(), "claude", "exit 1");
        let env = vec![(
            "PATH".to_string(),
            dir.path().to_string_lossy().into_owned(),
        )];
        let found = find(&["claude".into()], &env);
        assert!(found[0]["path"].is_string());
        assert!(found[0]["version"].is_null());
    }

    #[test]
    fn the_version_is_asked_in_the_temporary_folder() {
        let dir = tempfile::tempdir().unwrap();
        let said = dir.path().join("folder");
        // where to write is in the environment, so the script stays the same
        program(dir.path(), "claude", "pwd -P > \"$SAID\"; echo 1.0");
        let env = vec![
            (
                "PATH".to_string(),
                dir.path().to_string_lossy().into_owned(),
            ),
            ("SAID".to_string(), said.to_string_lossy().into_owned()),
        ];
        find(&["claude".into()], &env);
        let folder = std::fs::read_to_string(&said).unwrap();
        assert_eq!(
            Path::new(folder.trim()),
            std::env::temp_dir().canonicalize().unwrap()
        );
    }

    #[test]
    fn the_version_is_the_first_word_that_starts_with_a_digit() {
        assert_eq!(
            version_in("2.1.325 (Claude Code)").as_deref(),
            Some("2.1.325")
        );
        assert_eq!(
            version_in("codex-cli 0.98.0\nmore").as_deref(),
            Some("0.98.0")
        );
        assert_eq!(version_in("gemini v0.12.1").as_deref(), Some("0.12.1"));
        // the end of a sentence is not part of it
        assert_eq!(
            version_in("GitHub Copilot CLI 1.0.114.").as_deref(),
            Some("1.0.114")
        );
        assert_eq!(version_in("no version here"), None);
    }
}
