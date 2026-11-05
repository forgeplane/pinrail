//! Projects as an application operation: named sets of repositories the
//! person makes in the app to group the inbox and the sidebar by. Agents
//! and the CLI know nothing of them: a review or a session names only its
//! repository, and its project is looked up when it is read.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};

use crate::db::{Db, NotChanged};
use crate::error::Error;
use crate::events::{self, Bus, Notice};

/// The longest name a project can have.
pub const MAX_NAME: usize = 80;
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

    /// Creates a project with these repositories, moving any that are in
    /// another project. Returns it.
    pub fn create(&self, name: &str, repos: &[String]) -> Result<Value, Error> {
        let name = checked_name(name)?;
        let repos = repos
            .iter()
            .enumerate()
            .map(|(i, repo)| checked_repo(repo, &format!("/repos/{i}")))
            .collect::<Result<Vec<_>, _>>()?;
        let id = crate::id::next_project();
        let event_id = self
            .db
            .create_project(&id, &name, &repos, Utc::now())?
            .map_err(|e| refused(e, &id))?;
        self.announce(event_id);
        self.get(&id)
    }

    /// Renames a project. Returns it.
    pub fn rename(&self, id: &str, name: &str) -> Result<Value, Error> {
        let name = checked_name(name)?;
        let event_id = self
            .db
            .rename_project(id, &name)?
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
