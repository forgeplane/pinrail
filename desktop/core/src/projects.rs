//! Projects as an application operation: named sets of repositories the
//! person makes in the app to group the inbox and the sidebar by. Agents
//! and the CLI know nothing of them: a review or a session names only its
//! repository, and its project is looked up when it is read.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};

use crate::db::{Change, Db, Details, NotChanged};
use crate::error::Error;
use crate::events::{self, Bus, Notice};

/// The longest name a project can have.
pub const MAX_NAME: usize = 80;
/// The longest description a project can have.
pub const MAX_DESCRIPTION: usize = 280;
/// The colours a project's avatar can take, by the names the app draws them by.
pub const COLORS: [&str; 10] = [
    "gray", "red", "orange", "amber", "green", "teal", "blue", "indigo", "violet", "pink",
];
/// The longest emoji, or letters, a project's avatar takes in place of the
/// name's first letter: one emoji can run to several characters.
pub const MAX_EMOJI: usize = 8;
/// The longest repository name a project takes, as origins give it.
pub const MAX_REPO: usize = 200;

#[derive(Debug)]
pub struct Projects {
    db: Arc<Db>,
    bus: Bus,
}

impl Projects {
    pub(crate) fn new(db: Arc<Db>, bus: Bus) -> Self {
        Projects { db, bus }
    }

    /// Every project with its repositories and what they hold, by name,
    /// and the repositories in no project: `projects` and `loose`.
    pub fn listing(&self) -> Result<Value, Error> {
        let (projects, loose) = self.db.projects(Utc::now())?;
        Ok(json!({
            "projects": projects.iter().map(|p| p.to_json()).collect::<Vec<_>>(),
            "loose": loose.iter().map(|r| r.to_json()).collect::<Vec<_>>(),
        }))
    }

    /// One project, as the listing gives it.
    pub fn get(&self, id: &str) -> Result<Value, Error> {
        let (projects, _) = self.db.projects(Utc::now())?;
        projects
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.to_json())
            .ok_or_else(|| Error::NotFound(format!("project {id}")))
    }

    /// Creates a project with what `details` gives (a description, a
    /// colour, an emoji, each optional) and these repositories, moving any
    /// that are in another project. Returns it.
    pub fn create(&self, name: &str, details: &Details, repos: &[String]) -> Result<Value, Error> {
        let name = checked_name(name)?;
        let details = Details {
            description: checked(&details.description, checked_description)?,
            color: checked(&details.color, checked_color)?,
            emoji: checked(&details.emoji, checked_emoji)?,
        };
        let repos = repos
            .iter()
            .enumerate()
            .map(|(i, repo)| checked_repo(repo, &format!("/repos/{i}")))
            .collect::<Result<Vec<_>, _>>()?;
        let id = crate::id::next_project();
        let event_id = self
            .db
            .create_project(&id, &name, &details, &repos, Utc::now())?
            .map_err(|e| refused(e, &id))?;
        self.announce(event_id);
        self.get(&id)
    }

    /// Changes what `change` gives of a project: a field left out stays as
    /// it is, and an empty or removed description, colour or emoji goes.
    /// Returns it.
    pub fn update(&self, id: &str, change: &Change) -> Result<Value, Error> {
        let removable = |field: &Option<Option<String>>,
                         check: fn(&str) -> Result<Option<String>, Error>|
         -> Result<Option<Option<String>>, Error> {
            field
                .as_ref()
                .map(|value| checked(value, check))
                .transpose()
        };
        let change = Change {
            name: change.name.as_deref().map(checked_name).transpose()?,
            description: removable(&change.description, checked_description)?,
            color: removable(&change.color, checked_color)?,
            emoji: removable(&change.emoji, checked_emoji)?,
        };
        let event_id = self
            .db
            .update_project(id, &change)?
            .map_err(|e| refused(e, id))?;
        self.announce(event_id);
        self.get(id)
    }

    /// Deletes a project. Its repositories, reviews and sessions stay.
    pub fn delete(&self, id: &str) -> Result<(), Error> {
        let event_id = self.db.delete_project(id)?.map_err(|e| refused(e, id))?;
        self.announce(event_id);
        Ok(())
    }

    /// Puts a repository in a project, taking it out of the one it was in,
    /// or in no project with `None`.
    pub fn set_repo(&self, repo: &str, project: Option<&str>) -> Result<(), Error> {
        let repo = checked_repo(repo, "/repo")?;
        let event_id = self
            .db
            .set_repo_project(&repo, project, Utc::now())?
            .map_err(|e| refused(e, project.unwrap_or_default()))?;
        self.announce(event_id);
        Ok(())
    }

    fn announce(&self, event_id: i64) {
        self.bus.publish(Notice {
            event_id,
            kind: events::PROJECTS_CHANGED.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: None,
        });
    }
}

fn refused(error: NotChanged, id: &str) -> Error {
    match error {
        NotChanged::NameTaken => Error::invalid("/name", "another project has this name"),
        NotChanged::NoProject => Error::NotFound(format!("project {id}")),
    }
}

/// The name trimmed, or why it cannot be one.
fn checked_name(name: &str) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::invalid("/name", "is empty"));
    }
    if name.chars().count() > MAX_NAME {
        return Err(Error::invalid(
            "/name",
            format!("is longer than {MAX_NAME} characters"),
        ));
    }
    Ok(name.to_string())
}

/// The description trimmed, `None` when it is empty, or why it cannot be one.
fn checked_description(description: &str) -> Result<Option<String>, Error> {
    let description = description.trim();
    if description.chars().count() > MAX_DESCRIPTION {
        return Err(Error::invalid(
            "/description",
            format!("is longer than {MAX_DESCRIPTION} characters"),
        ));
    }
    Ok((!description.is_empty()).then(|| description.to_string()))
}

/// A field checked when it is given, `None` when it is not or is empty.
fn checked(
    value: &Option<String>,
    check: fn(&str) -> Result<Option<String>, Error>,
) -> Result<Option<String>, Error> {
    Ok(match value {
        Some(value) => check(value)?,
        None => None,
    })
}

/// A colour of the palette, `None` for none, or why it is not one.
fn checked_color(color: &str) -> Result<Option<String>, Error> {
    let color = color.trim();
    if color.is_empty() {
        return Ok(None);
    }
    if !COLORS.contains(&color) {
        return Err(Error::invalid(
            "/color",
            format!("is not one of {}", COLORS.join(", ")),
        ));
    }
    Ok(Some(color.to_string()))
}

/// The emoji or letters trimmed, `None` when empty, or why they cannot be one.
fn checked_emoji(emoji: &str) -> Result<Option<String>, Error> {
    let emoji = emoji.trim();
    if emoji.chars().count() > MAX_EMOJI {
        return Err(Error::invalid(
            "/emoji",
            format!("is longer than {MAX_EMOJI} characters: give one emoji or a letter or two"),
        ));
    }
    Ok((!emoji.is_empty()).then(|| emoji.to_string()))
}

/// The repository trimmed, or why it cannot be one.
fn checked_repo(repo: &str, path: &str) -> Result<String, Error> {
    let repo = repo.trim();
    if repo.is_empty() {
        return Err(Error::invalid(path, "is empty"));
    }
    if repo.chars().count() > MAX_REPO {
        return Err(Error::invalid(
            path,
            format!("is longer than {MAX_REPO} characters"),
        ));
    }
    Ok(repo.to_string())
}
