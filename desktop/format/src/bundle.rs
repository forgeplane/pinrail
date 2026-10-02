//! A bundle: the files of one release of a plugin, in one fixed layout, and
//! the hash that names it. The hash is the SHA-256 of a canonical listing of
//! the files, their hashes, sizes and paths, so it depends on what the files
//! hold and nothing else: not their order on disk, their times, their
//! permissions or the archive they came in.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// The first line of a listing, naming its format: a change to the rules
/// gives different hashes rather than ones that collide with these.
pub const FORMAT: &str = "pinrail-bundle-1";

/// The most files a bundle may hold.
pub const MAX_FILES: usize = 5_000;
/// The most a bundle's files may add up to: 100 MB.
pub const MAX_BYTES: u64 = 100 * 1024 * 1024;
/// The most one file of a bundle may be: 50 MB.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// The files a bundle holds at its top level, and nothing else.
const TOP_FILES: &[&str] = &["manifest.json", "icon.svg", "README.md", "LICENSE"];
/// The folders a bundle holds at its top level, with what is in them.
const TOP_FOLDERS: &[&str] = &["schemas", "view", "templates", "samples"];

/// Whether a top-level entry belongs in a bundle.
pub fn in_layout(top: &str) -> bool {
    TOP_FILES.contains(&top) || TOP_FOLDERS.contains(&top)
}

/// Whether a path in a source folder, relative to it with `/` between its
/// parts, is one a bundle taken from that folder holds: it is under the
/// layout, and neither it nor a folder on the way is hidden.
pub fn holds(path: &str) -> bool {
    let top = path.split('/').next().unwrap_or_default();
    in_layout(top) && !path.split('/').any(|part| part.starts_with('.'))
}

/// Where the files come from, which decides what happens to an entry
/// outside the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taken {
    /// A plugin's source folder: what is outside the layout (sources,
    /// `node_modules`, tests, configuration, dot files) is left behind.
    FromSource,
    /// A bundle as stored or downloaded: anything outside the layout is
    /// refused.
    AsBundle,
}

/// One file of a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Relative, separated by `/`, in Unicode normalization form C.
    pub path: String,
    pub size: u64,
    /// The SHA-256 of the file's bytes, in lowercase hex.
    pub sha256: String,
}

/// A bundle's files, sorted by path, and the hash of their listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub files: Vec<File>,
}

impl Listing {
    /// The listing of files given as paths and bytes, from a folder or an
    /// archive alike, under the rules every bundle follows.
    pub fn from_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        taken: Taken,
    ) -> Result<Listing, String> {
        let mut by_path: BTreeMap<String, File> = BTreeMap::new();
        // the same path in another case, which macOS and Windows take for
        // the same file
        let mut folded: BTreeMap<String, String> = BTreeMap::new();
        let mut total: u64 = 0;
        for (raw, bytes) in files {
            let path = normalized(raw)?;
            let top = path.split('/').next().unwrap_or_default();
            if !in_layout(top) {
                match taken {
                    Taken::FromSource => continue,
                    Taken::AsBundle => {
                        return Err(format!("{path}: not part of a bundle's layout"));
                    }
                }
            }
            // .DS_Store, .gitkeep and the like differ from one machine to
            // the next, and would change the hash
            if path.split('/').any(|part| part.starts_with('.')) {
                match taken {
                    Taken::FromSource => continue,
                    Taken::AsBundle => {
                        return Err(format!(
                            "{path}: a hidden file, which a bundle does not hold"
                        ));
                    }
                }
            }
            // a top-level name of a file only, or of a folder only
            if TOP_FILES.contains(&top) != !path.contains('/') {
                return Err(format!("{path}: not where the layout puts it"));
            }
            let size = bytes.len() as u64;
            if size > MAX_FILE_BYTES {
                return Err(format!(
                    "{path}: {size} bytes; a file is at most {MAX_FILE_BYTES}"
                ));
            }
            total += size;
            if total > MAX_BYTES {
                return Err(format!("the files add up to more than {MAX_BYTES} bytes"));
            }
            let lower = path.to_lowercase();
            if let Some(other) = folded.get(&lower) {
                return Err(format!(
                    "{path} and {other} differ only in case, and are one file on macOS and Windows"
                ));
            }
            folded.insert(lower, path.clone());
            let sha256 = hex(&Sha256::digest(bytes));
            by_path.insert(path.clone(), File { path, size, sha256 });
            if by_path.len() > MAX_FILES {
                return Err(format!("more than {MAX_FILES} files"));
            }
        }
        // a BTreeMap of Strings sorts by their UTF-8 bytes
        Ok(Listing {
            files: by_path.into_values().collect(),
        })
    }

    /// The listing of a folder: every regular file under it, read from
    /// disk. A symbolic link is refused, wherever it is.
    pub fn of_folder(dir: &Path, taken: Taken) -> Result<Listing, String> {
        let mut found: Vec<(String, Vec<u8>)> = Vec::new();
        walk(dir, dir, taken, &mut found)?;
        Listing::from_files(
            found
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
            taken,
        )
    }

    /// The canonical listing the hash is taken over: the format's line, then
    /// a line per file, `<sha256> <size> <path>`, each ending with `\n`.
    pub fn text(&self) -> String {
        let mut text = format!("{FORMAT}\n");
        for file in &self.files {
            text.push_str(&format!("{} {} {}\n", file.sha256, file.size, file.path));
        }
        text
    }

    /// The bundle's hash: the SHA-256 of its listing, in lowercase hex.
    pub fn hash(&self) -> String {
        hex(&Sha256::digest(self.text().as_bytes()))
    }

    /// What the bundle's files add up to, in bytes.
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// A path as a bundle records it: relative, `/` between its parts, in
/// Unicode normalization form C, with no empty, `.` or `..` part.
fn normalized(raw: &str) -> Result<String, String> {
    let path: String = raw.nfc().collect();
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return Err(format!(
            "{raw}: not a relative path with / between its parts"
        ));
    }
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("{raw}: a path with an empty, . or .. part"));
    }
    Ok(path)
}

fn walk(
    root: &Path,
    dir: &Path,
    taken: Taken,
    found: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let relative = relative(root, &path)?;
        // a source folder's entries outside the layout are never read, so
        // node_modules and the like cost nothing
        if taken == Taken::FromSource
            && ((dir == root && !in_layout(&relative))
                || entry.file_name().to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let kind = entry.file_type().map_err(|e| format!("{relative}: {e}"))?;
        if kind.is_symlink() {
            return Err(format!(
                "{relative}: a symbolic link; a bundle holds files only"
            ));
        }
        if kind.is_dir() {
            walk(root, &path, taken, found)?;
        } else if kind.is_file() {
            let bytes = fs::read(&path).map_err(|e| format!("{relative}: {e}"))?;
            found.push((relative, bytes));
        }
    }
    Ok(())
}

/// `path` relative to `root`, with `/` between its parts.
fn relative(root: &Path, path: &Path) -> Result<String, String> {
    let rest = path
        .strip_prefix(root)
        .map_err(|_| format!("{}: outside the folder", path.display()))?;
    let mut parts = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| format!("{}: a name that is not UTF-8", path.display()))?
                    .to_string(),
            ),
            _ => return Err(format!("{}: not a plain path", path.display())),
        }
    }
    Ok(parts.join("/"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plugin's files as a test lays them out: path and contents.
    const PLUGIN: &[(&str, &str)] = &[
        (
            "manifest.json",
            "{\"name\": \"fixture\", \"version\": \"1.0.0\"}\n",
        ),
        ("schemas/payload.schema.json", "{\"type\": \"object\"}\n"),
        ("schemas/decision.schema.json", "{\"type\": \"object\"}\n"),
        ("view/index.html", "<!doctype html>\n<p>fixture</p>\n"),
        ("icon.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n"),
    ];

    fn listing(files: &[(&str, &str)], taken: Taken) -> Result<Listing, String> {
        Listing::from_files(files.iter().map(|(p, t)| (*p, t.as_bytes())), taken)
    }

    fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let to = dir.path().join(path);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::write(to, text).unwrap();
        }
        dir
    }

    #[test]
    fn the_hash_of_a_fixed_plugin_is_a_known_value() {
        // the format's golden value: a change here is a change of format,
        // which FORMAT must name
        let listing = listing(PLUGIN, Taken::AsBundle).unwrap();
        assert_eq!(
            listing.text().lines().next(),
            Some("pinrail-bundle-1"),
            "{}",
            listing.text()
        );
        assert_eq!(
            listing
                .text()
                .lines()
                .map(|l| l.rsplit(' ').next().unwrap())
                .collect::<Vec<_>>(),
            [
                "pinrail-bundle-1",
                "icon.svg",
                "manifest.json",
                "schemas/decision.schema.json",
                "schemas/payload.schema.json",
                "view/index.html"
            ],
            "sorted by path"
        );
        // worked out from the spec's steps, by hand, not by this code
        assert_eq!(
            listing.hash(),
            "947be86c9601853b4573dfd48dafff6c75af9ee2c46fdca666be1be0ce096d7a",
            "{}",
            listing.text()
        );
    }

    #[test]
    fn the_same_files_hash_the_same_whatever_their_order_times_and_permissions() {
        let forward = listing(PLUGIN, Taken::AsBundle).unwrap();
        let mut reversed = PLUGIN.to_vec();
        reversed.reverse();
        assert_eq!(
            listing(&reversed, Taken::AsBundle).unwrap().hash(),
            forward.hash()
        );

        let dir = folder(PLUGIN);
        let file = dir.path().join("view/index.html");
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(old)
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            Listing::of_folder(dir.path(), Taken::AsBundle)
                .unwrap()
                .hash(),
            forward.hash(),
            "the folder on disk, with another time and the executable bit"
        );
    }

    #[test]
    fn any_change_to_the_files_changes_the_hash() {
        let base = listing(PLUGIN, Taken::AsBundle).unwrap().hash();
        type Files<'a> = Vec<(&'a str, &'a str)>;
        let changed = |edit: &dyn Fn(&mut Files)| {
            let mut files = PLUGIN.to_vec();
            edit(&mut files);
            listing(&files, Taken::AsBundle).unwrap().hash()
        };
        let variants = [
            changed(&|f| f[3].1 = "<!doctype html>\n<p>fixture.</p>\n"),
            changed(&|f| f[3].0 = "view/main.html"),
            changed(&|f| f.push(("view/app.js", ""))),
            changed(&|f| {
                f.remove(4);
            }),
            changed(&|f| f[3].1 = "<!doctype html>\r\n<p>fixture</p>\r\n"),
        ];
        for (i, hash) in variants.iter().enumerate() {
            assert_ne!(*hash, base, "variant {i}");
        }
    }

    #[test]
    fn a_zip_of_a_folder_hashes_the_same_as_the_folder() {
        use std::io::{Read, Write};
        let dir = folder(PLUGIN);
        let mut zipped = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zipped));
            for (path, text) in PLUGIN.iter().rev() {
                zip.start_file(*path, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(text.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
        let mut files = Vec::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            files.push((entry.name().to_string(), bytes));
        }
        let from_zip = Listing::from_files(
            files.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
            Taken::AsBundle,
        )
        .unwrap();
        assert_eq!(
            from_zip.hash(),
            Listing::of_folder(dir.path(), Taken::AsBundle)
                .unwrap()
                .hash()
        );
    }

    #[test]
    fn paths_are_normalized_and_unsafe_ones_refused() {
        // é as e and a combining accent (NFD) is é as one character (NFC)
        let nfd = listing(&[("view/cafe\u{301}.txt", "x")], Taken::AsBundle).unwrap();
        let nfc = listing(&[("view/caf\u{e9}.txt", "x")], Taken::AsBundle).unwrap();
        assert_eq!(nfd.files[0].path, "view/caf\u{e9}.txt");
        assert_eq!(nfd.hash(), nfc.hash());

        for bad in [
            "view/../manifest.json",
            "view/./a.js",
            "/view/a.js",
            "view//a.js",
            "view\\a.js",
        ] {
            let refused = listing(&[(bad, "x")], Taken::AsBundle).unwrap_err();
            assert!(refused.starts_with(bad), "{bad}: {refused}");
        }
        let refused = listing(
            &[("view/App.js", "a"), ("view/app.js", "b")],
            Taken::AsBundle,
        )
        .unwrap_err();
        assert!(refused.contains("differ only in case"), "{refused}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_refused() {
        let dir = folder(PLUGIN);
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join("view/hosts")).unwrap();
        let refused = Listing::of_folder(dir.path(), Taken::FromSource).unwrap_err();
        assert_eq!(
            refused,
            "view/hosts: a symbolic link; a bundle holds files only"
        );
    }

    #[test]
    fn what_is_outside_the_layout_is_left_behind_from_a_source_and_refused_in_a_bundle() {
        let mut source = PLUGIN.to_vec();
        source.extend([
            ("package.json", "{}"),
            ("src/main.ts", "x"),
            ("node_modules/a/index.js", "x"),
            (".env", "SECRET=1"),
            ("example.json", "{}"),
        ]);
        let dir = folder(&source);
        let taken = Listing::of_folder(dir.path(), Taken::FromSource).unwrap();
        assert_eq!(
            taken.hash(),
            listing(PLUGIN, Taken::AsBundle).unwrap().hash()
        );

        let refused = listing(&source, Taken::AsBundle).unwrap_err();
        assert_eq!(refused, "package.json: not part of a bundle's layout");
        // a top-level name in the wrong kind: a folder called manifest.json,
        // a file called view
        assert!(listing(&[("manifest.json/a", "x")], Taken::AsBundle).is_err());
        assert!(listing(&[("view", "x")], Taken::AsBundle).is_err());
    }

    #[test]
    fn a_hidden_file_is_left_behind_from_a_source_and_refused_in_a_bundle() {
        let mut source = PLUGIN.to_vec();
        source.extend([
            ("view/.DS_Store", "x"),
            ("samples/.cache/a.json", "{}"),
            ("view/assets/.gitkeep", ""),
        ]);
        let dir = folder(&source);
        let taken = Listing::of_folder(dir.path(), Taken::FromSource).unwrap();
        assert_eq!(
            taken.hash(),
            listing(PLUGIN, Taken::AsBundle).unwrap().hash()
        );

        let refused = listing(&[("view/.DS_Store", "x")], Taken::AsBundle).unwrap_err();
        assert_eq!(
            refused,
            "view/.DS_Store: a hidden file, which a bundle does not hold"
        );
    }

    #[test]
    fn a_path_is_held_when_under_the_layout_and_not_hidden() {
        for held in [
            "manifest.json",
            "view/index.html",
            "samples/a/b.glb",
            "LICENSE",
        ] {
            assert!(holds(held), "{held} left out");
        }
        for left in [
            ".env",
            ".git/config",
            "node_modules/a/index.js",
            "src/main.ts",
            "package.json",
            "notes.txt",
            "view/.DS_Store",
            "samples/.cache/a.json",
        ] {
            assert!(!holds(left), "{left} held");
        }
    }

    #[test]
    fn the_limits_hold_at_the_limit_and_refuse_one_over() {
        let big = vec![0u8; MAX_FILE_BYTES as usize];
        let at = Listing::from_files([("view/big.bin", big.as_slice())], Taken::AsBundle);
        assert!(at.is_ok());
        let over = vec![0u8; MAX_FILE_BYTES as usize + 1];
        let refused =
            Listing::from_files([("view/big.bin", over.as_slice())], Taken::AsBundle).unwrap_err();
        assert!(refused.contains("a file is at most"), "{refused}");

        // two files at the file limit are the total limit; one byte more is over
        let names: Vec<String> = (0..3).map(|i| format!("view/{i}.bin")).collect();
        let last = vec![0u8; 1];
        let three = [
            (names[0].as_str(), big.as_slice()),
            (names[1].as_str(), big.as_slice()),
            (names[2].as_str(), last.as_slice()),
        ];
        let refused = Listing::from_files(three, Taken::AsBundle).unwrap_err();
        assert!(refused.contains("add up to more than"), "{refused}");
        assert!(Listing::from_files(three[..2].iter().copied(), Taken::AsBundle).is_ok());

        let names: Vec<String> = (0..=MAX_FILES).map(|i| format!("view/{i}.txt")).collect();
        let at = Listing::from_files(
            names[..MAX_FILES].iter().map(|n| (n.as_str(), &b""[..])),
            Taken::AsBundle,
        );
        assert!(at.is_ok());
        let refused = Listing::from_files(
            names.iter().map(|n| (n.as_str(), &b""[..])),
            Taken::AsBundle,
        )
        .unwrap_err();
        assert_eq!(refused, format!("more than {MAX_FILES} files"));
    }
}
