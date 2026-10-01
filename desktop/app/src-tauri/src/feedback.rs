//! Feedback the person sends from the app, passed on to the Pinrail team's
//! feedback service. The dialog builds the form, the subject, the message,
//! the reply address and the attached files, and hands it over as it would
//! go over HTTP; this adds what the app knows of itself and sends it.

use std::time::Duration;

/// Where feedback goes; `PINRAIL_FEEDBACK_URL` points a development build or
/// a test elsewhere.
const ENDPOINT: &str = "https://api.pinrail.dev/v1/feedback";

/// How long the upload of a report and its files may take.
const TIMEOUT: Duration = Duration::from_secs(60);

pub fn endpoint() -> String {
    std::env::var("PINRAIL_FEEDBACK_URL").unwrap_or_else(|_| ENDPOINT.to_string())
}

/// Sends a multipart form to `url`, with the app's version and system in
/// headers. The error is a sentence to show the person.
pub fn send(url: &str, content_type: &str, body: &[u8]) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .post(url)
        .header("content-type", content_type)
        .header(
            "x-pinrail-client",
            format!("pinrail/{}", env!("CARGO_PKG_VERSION")),
        )
        .header(
            "x-pinrail-platform",
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        )
        .header("x-pinrail-os", os_version().unwrap_or_default())
        .send(body)
        .map_err(|_| {
            "Pinrail could not reach the feedback service. Check your connection and try again."
                .to_string()
        })?;
    if response.status().is_success() {
        return Ok(());
    }
    // the service says what was wrong, in a sentence
    let said = response
        .body_mut()
        .read_json::<serde_json::Value>()
        .ok()
        .and_then(|answer| answer["message"].as_str().map(str::to_string));
    Err(match said {
        Some(message) => format!("The feedback was not sent: {message}."),
        None => format!(
            "The feedback was not sent: the service answered {}.",
            response.status()
        ),
    })
}

/// The operating system's name and version, as people know it: "macOS
/// 15.4.1", or on Linux the distribution and the kernel. `None` when the
/// system does not say.
pub fn os_version() -> Option<String> {
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        let version = String::from_utf8(out.stdout).ok()?.trim().to_string();
        return (!version.is_empty()).then(|| format!("macOS {version}"));
    }
    if cfg!(target_os = "linux") {
        let release = std::fs::read_to_string("/etc/os-release").ok();
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").ok();
        return linux_version(release.as_deref(), kernel.as_deref());
    }
    None
}

/// "Arch Linux, kernel 6.10.3": the distribution's name from os-release
/// and the kernel's release.
fn linux_version(os_release: Option<&str>, kernel: Option<&str>) -> Option<String> {
    let name = os_release.and_then(|text| {
        let field = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
                .map(|value| value.trim().trim_matches('"').to_string())
                .filter(|value| !value.is_empty())
        };
        field("PRETTY_NAME").or_else(|| field("NAME"))
    });
    let kernel = kernel.map(str::trim).filter(|k| !k.is_empty());
    match (name, kernel) {
        (Some(name), Some(kernel)) => Some(format!("{name}, kernel {kernel}")),
        (Some(name), None) => Some(name),
        (None, Some(kernel)) => Some(format!("Linux, kernel {kernel}")),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// A feedback service on a local port that answers once with `status`
    /// and `body`, and hands back the request it got.
    fn service(status: &str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/feedback", listener.local_addr().unwrap());
        let status = status.to_string();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut sent = vec![0; length];
            reader.read_exact(&mut sent).unwrap();
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            head + &String::from_utf8_lossy(&sent)
        });
        (url, handle)
    }

    #[test]
    fn a_report_goes_as_it_was_built_with_the_apps_version_and_system() {
        let (url, request) = service("200 OK", r#"{"ok":true}"#);
        let body =
            b"--b\r\ncontent-disposition: form-data; name=\"message\"\r\n\r\nhi\r\n--b--\r\n";
        send(&url, "multipart/form-data; boundary=b", body).unwrap();
        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("post /v1/feedback "), "{request}");
        assert!(
            request.contains("content-type: multipart/form-data; boundary=b"),
            "{request}"
        );
        assert!(
            request.contains(&format!(
                "x-pinrail-client: pinrail/{}",
                env!("CARGO_PKG_VERSION")
            )),
            "{request}"
        );
        assert!(
            request.contains(&format!(
                "x-pinrail-platform: {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
            "{request}"
        );
        assert!(
            request.ends_with("name=\"message\"\r\n\r\nhi\r\n--b--\r\n"),
            "{request}"
        );
    }

    #[test]
    fn the_report_names_the_systems_version() {
        let (url, request) = service("200 OK", r#"{"ok":true}"#);
        send(&url, "text/plain", b"").unwrap();
        let request = request.join().unwrap();
        let line = request
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("x-pinrail-os: ")
                    .map(str::to_string)
            })
            .expect("the x-pinrail-os header");
        if cfg!(target_os = "macos") {
            // macOS 13 to 15, then 26 and on
            let version = line.strip_prefix("macos ").unwrap_or_default();
            assert!(version.starts_with(|c: char| c.is_ascii_digit()), "{line}");
        }
        if cfg!(target_os = "linux") {
            assert!(line.contains(", kernel "), "{line}");
        }
    }

    #[test]
    fn a_linux_system_is_named_by_its_distribution_and_kernel() {
        let arch = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n";
        assert_eq!(
            linux_version(Some(arch), Some("6.10.3-arch1-1\n")).as_deref(),
            Some("Arch Linux, kernel 6.10.3-arch1-1")
        );
        let bare = "NAME=Fedora Linux\nVERSION_ID=44\n";
        assert_eq!(
            linux_version(Some(bare), None).as_deref(),
            Some("Fedora Linux")
        );
        assert_eq!(
            linux_version(None, Some("6.8.0")).as_deref(),
            Some("Linux, kernel 6.8.0")
        );
        assert_eq!(linux_version(None, None), None);
    }

    #[test]
    fn a_refusal_is_told_in_the_services_words() {
        let (url, _) = service(
            "413 Payload Too Large",
            r#"{"error":"too_large","message":"the attached files may weigh 10 MB together"}"#,
        );
        assert_eq!(
            send(&url, "multipart/form-data; boundary=b", b"--b--\r\n"),
            Err("The feedback was not sent: the attached files may weigh 10 MB together.".into())
        );
    }

    #[test]
    fn a_service_that_cannot_be_reached_is_said_plainly() {
        // nothing listens on port 9
        let error = send("http://127.0.0.1:9/v1/feedback", "text/plain", b"").unwrap_err();
        assert!(
            error.contains("could not reach the feedback service"),
            "{error}"
        );
    }
}
