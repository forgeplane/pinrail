//! The official plugins the app carries, offered to the person to install.
//! Nothing in the catalog is installed until the person chooses it.
//!
//! The catalog is an index in the registry's shape: each plugin is known by
//! its id, `forgeplane/<name>`, and its version, and an install from it
//! records the same as an install from the registry's index will. The
//! registry later adds a second catalog, fetched rather than carried, and
//! the version a plugin can be updated to is the highest any of them offers.

use include_dir::{Dir, include_dir};
use pinrail_format::bundle::{Listing, Taken};
use serde_json::{Map, Value, json};

use super::bundles::Files;

/// The plugins the app carries. They live in `plugins/` with the test
/// fixtures; build.rs copies the catalog's bundles here for the binary.
static CATALOG: Dir = include_dir!("$OUT_DIR/catalog");

/// The publisher of the plugins the app carries.
pub const PUBLISHER: &str = "forgeplane";

/// The repository the official plugins come from.
const REPOSITORY: &str = "https://github.com/forgeplane/pinrail";

/// The plugins the setup wizard selects.
const RECOMMENDED: &[&str] = &["list", "feedback"];

/// One plugin a catalog offers, with the files an install stores.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub version: String,
    /// The plugin's files, paths relative to the plugin, as a bundle holds
    /// them.
    pub files: Files,
    /// The hash of those files as a bundle: what an install of the entry
    /// records as its bundle.
    pub hash: String,
    manifest: Map<String, Value>,
    icon: Option<String>,
}

impl Entry {
    fn of(files: Files) -> Result<Entry, String> {
        let manifest = files
            .iter()
            .find(|(path, _)| path == "manifest.json")
            .and_then(|(_, bytes)| serde_json::from_slice::<Value>(bytes).ok())
            .and_then(|value| value.as_object().cloned())
            .ok_or("no manifest.json")?;
        let name = manifest
            .get("name")
            .and_then(Value::as_str)
            .ok_or("no name in manifest.json")?
            .to_string();
        let version = manifest
            .get("version")
            .and_then(super::version_of)
            .map(|(version, _)| version)
            .ok_or("no version in manifest.json")?;
        let hash = Listing::from_files(
            files.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
            Taken::AsBundle,
        )?
        .hash();
        let icon = files
            .iter()
            .find(|(path, _)| path == pinrail_format::manifest::ICON)
            .and_then(|(path, bytes)| {
                let text = String::from_utf8_lossy(bytes);
                pinrail_format::manifest::svg_markup(path, &text).ok()
            });
        Ok(Entry {
            name,
            version,
            files,
            hash,
            manifest,
            icon,
        })
    }

    /// The Pinrail version the plugin needs, when this one is older.
    pub fn needs(&self) -> Option<String> {
        let needed = self.manifest.get("pinrail")?.as_str()?;
        let needed = needed.trim_start_matches(">=").trim();
        let this = env!("CARGO_PKG_VERSION");
        (pinrail_format::semver(this) < pinrail_format::semver(needed)).then(|| needed.to_string())
    }

    /// The registry's id: `<publisher>/<name>`.
    pub fn id(&self) -> String {
        format!("{PUBLISHER}/{}", self.name)
    }

    /// The entry as the registry's compiled index lists a plugin. The
    /// catalog's zips are its files, so `url` is null and `sha256` is the
    /// bundle's hash.
    pub fn to_json(&self) -> Value {
        let field = |key: &str| self.manifest.get(key).cloned().unwrap_or(Value::Null);
        json!({
            "id": self.id(),
            "publisher": PUBLISHER,
            "name": self.name,
            "official": true,
            "recommended": RECOMMENDED.contains(&self.name.as_str()),
            "repository": REPOSITORY,
            "version": self.version,
            "url": null,
            "sha256": self.hash,
            "size": self.files.iter().map(|(_, b)| b.len()).sum::<usize>(),
            "pinrail": field("pinrail"),
            "title": self.manifest.get("title").cloned().unwrap_or_else(|| self.name.clone().into()),
            "description": field("description"),
            "use_when": field("use_when"),
            "icon": self.icon,
            "attachments": field("attachments"),
        })
    }
}

/// The plugins one source offers, by name.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: Vec<Entry>,
}

impl Catalog {
    /// The plugins this app carries.
    pub fn builtin() -> Catalog {
        Catalog::of(carried())
    }

    /// A catalog of these plugins' files, each by its folder. A plugin
    /// whose files are not a bundle is left out.
    pub fn of(plugins: Vec<(String, Files)>) -> Catalog {
        let mut entries: Vec<Entry> = plugins
            .into_iter()
            .filter_map(|(_, files)| Entry::of(files).ok())
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Catalog { entries }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

/// The entry for `id`, `forgeplane/<name>` or the bare name, at the highest
/// version any of the catalogs offers.
pub fn best<'a>(catalogs: &'a [Catalog], id: &str) -> Option<&'a Entry> {
    let name = match id.split_once('/') {
        Some((PUBLISHER, name)) => name,
        Some(_) => return None,
        None => id,
    };
    catalogs
        .iter()
        .flat_map(|c| c.entries.iter())
        .filter(|e| e.name == name)
        .max_by(|a, b| pinrail_format::semver(&a.version).cmp(&pinrail_format::semver(&b.version)))
}

/// Every plugin the catalogs offer, each at its highest version, by name.
pub fn listing(catalogs: &[Catalog]) -> Vec<&Entry> {
    let mut names: Vec<&str> = catalogs
        .iter()
        .flat_map(|c| c.entries.iter().map(|e| e.name.as_str()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| best(catalogs, name))
        .collect()
}

/// The files of each plugin the app carries, by its folder: paths relative
/// to the plugin, and their bytes.
pub(crate) fn carried() -> Vec<(String, Files)> {
    fn files(dir: &Dir, root: &std::path::Path, out: &mut Files) {
        for file in dir.files() {
            let path = file.path().strip_prefix(root).unwrap_or(file.path());
            out.push((
                path.to_string_lossy().replace('\\', "/"),
                file.contents().to_vec(),
            ));
        }
        for child in dir.dirs() {
            files(child, root, out);
        }
    }
    CATALOG
        .dirs()
        .map(|plugin| {
            let mut out = Vec::new();
            files(plugin, plugin.path(), &mut out);
            (plugin.path().to_string_lossy().into_owned(), out)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `list` from the app's catalog at another version.
    fn list_at(version: &str) -> Catalog {
        let files = carried()
            .into_iter()
            .find(|(folder, _)| folder == "list")
            .unwrap()
            .1
            .into_iter()
            .map(|(path, bytes)| match path.as_str() {
                "manifest.json" => {
                    let text = String::from_utf8(bytes).unwrap();
                    let bumped = text.replace("\"1.0.0\"", &format!("\"{version}\""));
                    (path, bumped.into_bytes())
                }
                _ => (path, bytes),
            })
            .collect();
        Catalog::of(vec![("list".into(), files)])
    }

    #[test]
    fn the_app_carries_list_and_feedback_as_official_plugins() {
        let catalog = Catalog::builtin();
        let rows: Vec<Value> = catalog.entries().iter().map(Entry::to_json).collect();
        let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["forgeplane/feedback", "forgeplane/list"]);
        for row in &rows {
            assert_eq!(row["official"], true);
            assert_eq!(row["recommended"], true);
            assert!(row["icon"].as_str().unwrap().starts_with("<svg"));
            assert!(row["use_when"].is_string());
            assert_eq!(row["sha256"].as_str().unwrap().len(), 64);
        }
    }

    /// What the app carries is what a plugin's folder holds as a bundle.
    #[test]
    fn a_carried_plugin_ships_its_bundle_and_nothing_else() {
        for (folder, files) in carried() {
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../plugins")
                .join(&folder);
            let bundle = Listing::of_folder(&source, Taken::FromSource).unwrap();
            let shipped = Listing::from_files(
                files.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
                Taken::AsBundle,
            );
            assert_eq!(shipped, Ok(bundle), "{folder}");
        }
    }

    #[test]
    fn the_highest_version_any_catalog_offers_wins() {
        let catalogs = [Catalog::builtin(), list_at("1.2.0"), list_at("0.9.0")];
        assert_eq!(best(&catalogs, "list").unwrap().version, "1.2.0");
        assert_eq!(best(&catalogs, "forgeplane/list").unwrap().version, "1.2.0");
        assert_eq!(best(&catalogs, "feedback").unwrap().version, "1.0.0");
        assert!(best(&catalogs, "acme/list").is_none());
        assert!(best(&catalogs, "nothing").is_none());
        let names: Vec<(&str, &str)> = listing(&catalogs)
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();
        assert_eq!(names, [("feedback", "1.0.0"), ("list", "1.2.0")]);
    }
}
