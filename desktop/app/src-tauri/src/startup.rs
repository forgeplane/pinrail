//! What to tell the person when the app cannot start: the two things it
//! cannot do without, its port and its data directory. Opened from the
//! Dock or a launcher, stderr is never seen, so this goes in a dialog,
//! which shows plain text: no markdown.

use std::path::Path;

use serde_json::Value;

/// The server's port is taken. `running` is the `server.json` in the data
/// directory, which names the Pinrail server that holds it, if one does.
pub fn port_in_use(port: u16, running: Option<&Value>) -> String {
    let pinrail = running
        .filter(|info| info["port"].as_u64() == Some(u64::from(port)))
        .and_then(|info| info["pid"].as_u64());
    match pinrail {
        Some(pid) => format!(
            "Another Pinrail server is already running on port {port} (process {pid}), \
             probably one the pinrail command started. Stop it with kill {pid}, \
             then open Pinrail again."
        ),
        None => format!(
            "Port {port} is in use by another program, so Pinrail cannot start its server. \
             Quit that program, or set PINRAIL_PORT to another port, then open Pinrail again."
        ),
    }
}

/// The data directory cannot be opened.
pub fn data_dir(dir: &Path, error: &str) -> String {
    format!(
        "Pinrail cannot open its data directory, {}: {error}. \
         Check that it is a folder you can write to, then open Pinrail again.",
        dir.display()
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_pinrail_server_on_the_port_is_named_with_how_to_stop_it() {
        let running = json!({"url": "http://127.0.0.1:4747", "port": 4747, "pid": 4242});
        let message = port_in_use(4747, Some(&running));
        assert!(message.contains("Another Pinrail server"), "{message}");
        assert!(message.contains("kill 4242"), "{message}");
    }

    #[test]
    fn another_program_on_the_port_points_at_pinrail_port() {
        // no server.json, or one for another port: not ours
        for running in [None, Some(json!({"port": 4800, "pid": 4242}))] {
            let message = port_in_use(4747, running.as_ref());
            assert!(message.contains("in use by another program"), "{message}");
            assert!(message.contains("PINRAIL_PORT"), "{message}");
        }
    }

    #[test]
    fn a_data_directory_that_cannot_be_opened_is_named() {
        let message = data_dir(Path::new("/tmp/nope"), "Not a directory");
        assert!(message.contains("/tmp/nope: Not a directory"), "{message}");
    }
}
