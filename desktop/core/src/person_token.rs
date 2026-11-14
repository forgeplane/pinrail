//! `<data dir>/person-token`: the person token, for the `pinrail` command
//! to start, stop and attach to agents. It is written only while
//! `/runtime/cli_control` is on, because every program of the same user
//! can read it, agents included, and the token allows every person action.

use std::io::Write;
use std::path::PathBuf;

use crate::Config;

/// The setting that lets the CLI act as the person.
pub const SETTING: &str = "/runtime/cli_control";

pub const FILE_NAME: &str = "person-token";

pub fn path(config: &Config) -> PathBuf {
    config.data_dir.join(FILE_NAME)
}

/// Writes the file while `on`, readable by its owner only, and removes it
/// otherwise.
pub fn sync(config: &Config, on: bool) -> std::io::Result<()> {
    if !on {
        remove(config);
        return Ok(());
    }
    std::fs::create_dir_all(&config.data_dir)?;
    let file = path(config);
    let partial = config.data_dir.join(format!("{FILE_NAME}.partial"));
    let _ = std::fs::remove_file(&partial);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&partial)?;
    out.write_all(config.person_token.as_bytes())?;
    out.sync_all()?;
    std::fs::rename(partial, file)
}

/// Removes the file if it holds this run's token, so that a core that
/// quits leaves a newer core's file alone.
pub fn remove(config: &Config) {
    let file = path(config);
    if std::fs::read_to_string(&file).is_ok_and(|token| token.trim() == config.person_token) {
        let _ = std::fs::remove_file(file);
    }
}
