//! Projects: named sets of repositories the person groups the inbox and the
//! sidebar by. A repository is in at most one project. Reviews and sessions
//! keep only their repository, and their project is looked up when they are
//! read, so a repository added to a project brings its past work with it.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::Db;
use super::events::insert_event;
use crate::reviews::parse_datetime;

/// A project as reviews and sessions name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
    /// its avatar's colour and emoji, when the person chose them
    pub color: Option<String>,
    pub emoji: Option<String>,
    /// the hash of its avatar's PNG, when the person chose one
    pub image: Option<String>,
}

impl ProjectRef {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "color": self.color,
            "emoji": self.emoji,
            "image": self.image,
        })
    }

    /// The project as `project_json` gives it, or `None` for a repository in
    /// no project.
    pub(super) fn from_column(json: Option<String>) -> Option<Self> {
        let value: Value = serde_json::from_str(&json?).ok()?;
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(ProjectRef {
            id: text("id")?,
            name: text("name")?,
            color: text("color"),
            emoji: text("emoji"),
            image: text("image"),
        })
    }
}

/// A repository the app knows, from a review, a session or a project, with
/// what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct RepoRow {
    pub repo: String,
    pub project_id: Option<String>,
    /// its sessions that are not archived
    pub sessions: usize,
    /// its questions and reviews that wait for the person, newest rounds only
    pub waiting: usize,
    /// its latest session activity or review
    pub last_at: Option<DateTime<Utc>>,
}

impl RepoRow {
    pub fn to_json(&self) -> Value {
        json!({
            "repo": self.repo,
            "sessions": self.sessions,
            "waiting": self.waiting,
            "last_at": self.last_at.map(crate::reviews::iso),
        })
    }
}

/// A project with its repositories.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub emoji: Option<String>,
    pub image: Option<String>,
    pub created_at: DateTime<Utc>,
    pub repos: Vec<RepoRow>,
}

impl ProjectRow {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "color": self.color,
            "emoji": self.emoji,
            "image": self.image,
            "created_at": crate::reviews::iso(self.created_at),
            "repos": self.repos.iter().map(RepoRow::to_json).collect::<Vec<_>>(),
            "sessions": self.repos.iter().map(|r| r.sessions).sum::<usize>(),
            "waiting": self.repos.iter().map(|r| r.waiting).sum::<usize>(),
            "last_at": self.repos.iter().filter_map(|r| r.last_at).max().map(crate::reviews::iso),
        })
    }
}

/// What a project says about itself besides its name: each `None` when the
/// person gave none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub description: Option<String>,
    pub color: Option<String>,
    pub emoji: Option<String>,
}

/// A change to a project: `None` leaves a field as it is, and
/// `Some(None)` removes the description, the colour or the emoji.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub color: Option<Option<String>>,
    pub emoji: Option<Option<String>>,
}

/// Why a change to the projects was not made.
#[derive(Debug, PartialEq, Eq)]
pub enum NotChanged {
    /// another project has this name, whatever its case
    NameTaken,
    /// no project has this id
    NoProject,
}

/// The SQL for the project of a repository given as `repo`, as a JSON
/// object that `ProjectRef::from_column` reads, or NULL.
pub(super) fn project_json(repo: &str) -> String {
    format!(
        "(SELECT json_object('id', p.id, 'name', p.name, 'color', p.color, 'emoji', p.emoji, \
         'image', p.image) \
         FROM project_repos pr JOIN projects p ON p.id = pr.project_id WHERE pr.repo = {repo})"
    )
}

/// The SQL that keeps rows whose repository, given as `repo`, is in the
/// project given as the parameter `?n`.
pub(super) fn in_project(repo: &str, n: usize) -> String {
    format!("{repo} IN (SELECT pr.repo FROM project_repos pr WHERE pr.project_id = ?{n})")
}

/// Records that the projects changed, for the live updates.
fn changed(conn: &Connection) -> rusqlite::Result<i64> {
    insert_event(
        conn,
        None,
        crate::events::PROJECTS_CHANGED,
        None,
        &json!({}),
    )
}

fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(e, _)
        if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
}

impl Db {
    /// Every project, by name, with its repositories; and the repositories
    /// in no project. `now` decides which reviews still wait.
    pub fn projects(
        &self,
        now: DateTime<Utc>,
    ) -> rusqlite::Result<(Vec<ProjectRow>, Vec<RepoRow>)> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "WITH known AS (
               SELECT repo FROM sessions WHERE COALESCE(repo, '') <> ''
               UNION SELECT json_extract(origin, '$.repo') FROM reviews
                 WHERE COALESCE(json_extract(origin, '$.repo'), '') <> ''
               UNION SELECT repo FROM project_repos
             )
             SELECT k.repo, pr.project_id,
               (SELECT COUNT(*) FROM sessions s WHERE s.repo = k.repo AND s.archived_at IS NULL),
               (SELECT COUNT(*) FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id
                WHERE json_extract(r.origin, '$.repo') = k.repo AND o.review_id IS NULL
                  AND (r.expires_at IS NULL OR r.expires_at > ?1)
                  AND NOT EXISTS (SELECT 1 FROM reviews n WHERE n.revises = r.id)),
               (SELECT MAX(s.last_at) FROM sessions s WHERE s.repo = k.repo),
               (SELECT MAX(r.created_at) FROM reviews r WHERE json_extract(r.origin, '$.repo') = k.repo)
             FROM known k LEFT JOIN project_repos pr ON pr.repo = k.repo
             ORDER BY k.repo",
        )?;
        let repos = stmt
            .query_map(params![crate::reviews::iso(now)], |row| {
                let time = |i: usize| -> rusqlite::Result<Option<DateTime<Utc>>> {
                    Ok(row
                        .get::<_, Option<String>>(i)?
                        .and_then(|t| parse_datetime(&t)))
                };
                Ok(RepoRow {
                    repo: row.get(0)?,
                    project_id: row.get(1)?,
                    sessions: row.get::<_, i64>(2)? as usize,
                    waiting: row.get::<_, i64>(3)? as usize,
                    last_at: time(4)?.max(time(5)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description, color, emoji, image, created_at FROM projects ORDER BY name COLLATE NOCASE",
        )?;
        let mut projects = stmt
            .query_map([], |row| {
                Ok(ProjectRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    color: row.get(3)?,
                    emoji: row.get(4)?,
                    image: row.get(5)?,
                    created_at: parse_datetime(&row.get::<_, String>(6)?).unwrap_or_default(),
                    repos: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut loose = Vec::new();
        for repo in repos {
            match projects
                .iter_mut()
                .find(|p| Some(&p.id) == repo.project_id.as_ref())
            {
                Some(project) => project.repos.push(repo),
                None => loose.push(repo),
            }
        }
        Ok((projects, loose))
    }

    /// The project a repository is in, if any.
    pub fn project_of_repo(&self, repo: &str) -> rusqlite::Result<Option<ProjectRef>> {
        let conn = self.conn();
        let json: Option<Option<String>> = conn
            .query_row(
                &format!("SELECT {}", project_json("?1")),
                params![repo],
                |row| row.get(0),
            )
            .optional()?;
        Ok(ProjectRef::from_column(json.flatten()))
    }

    /// Creates a project with these repositories, moving any that are in
    /// another project. Returns its event's id.
    pub fn create_project(
        &self,
        id: &str,
        name: &str,
        details: &Details,
        repos: &[String],
        at: DateTime<Utc>,
    ) -> rusqlite::Result<Result<i64, NotChanged>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let at = crate::reviews::iso(at);
        match tx.execute(
            "INSERT INTO projects (id, name, description, color, emoji, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                name,
                details.description,
                details.color,
                details.emoji,
                at
            ],
        ) {
            Err(error) if is_unique_violation(&error) => return Ok(Err(NotChanged::NameTaken)),
            inserted => inserted?,
        };
        for repo in repos {
            move_repo(&tx, repo, Some(id), &at)?;
        }
        let event_id = changed(&tx)?;
        tx.commit()?;
        Ok(Ok(event_id))
    }

    /// Sets the PNG a project's avatar shows, by its blob's hash, or removes
    /// it with `None`. Returns its event's id.
    pub fn set_project_image(
        &self,
        id: &str,
        image: Option<&str>,
    ) -> rusqlite::Result<Result<i64, NotChanged>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if tx.execute(
            "UPDATE projects SET image = ?2 WHERE id = ?1",
            params![id, image],
        )? == 0
        {
            return Ok(Err(NotChanged::NoProject));
        }
        let event_id = changed(&tx)?;
        tx.commit()?;
        Ok(Ok(event_id))
    }

    /// The hash of a project's PNG, if it has one; `None` when there is no
    /// such project.
    pub fn project_image(&self, id: &str) -> rusqlite::Result<Option<Option<String>>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT image FROM projects WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
    }

    /// Changes what `change` gives of a project. Returns its event's id.
    pub fn update_project(
        &self,
        id: &str,
        change: &Change,
    ) -> rusqlite::Result<Result<i64, NotChanged>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        match tx.execute(
            "UPDATE projects SET name = COALESCE(?2, name),
               description = CASE WHEN ?3 THEN ?4 ELSE description END,
               color = CASE WHEN ?5 THEN ?6 ELSE color END,
               emoji = CASE WHEN ?7 THEN ?8 ELSE emoji END
             WHERE id = ?1",
            params![
                id,
                change.name,
                change.description.is_some(),
                change.description.clone().flatten(),
                change.color.is_some(),
                change.color.clone().flatten(),
                change.emoji.is_some(),
                change.emoji.clone().flatten(),
            ],
        ) {
            Err(error) if is_unique_violation(&error) => return Ok(Err(NotChanged::NameTaken)),
            Ok(0) => return Ok(Err(NotChanged::NoProject)),
            updated => updated?,
        };
        let event_id = changed(&tx)?;
        tx.commit()?;
        Ok(Ok(event_id))
    }

    /// Deletes a project; its repositories are in no project after it.
    /// Returns its event's id.
    pub fn delete_project(&self, id: &str) -> rusqlite::Result<Result<i64, NotChanged>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if tx.execute("DELETE FROM projects WHERE id = ?1", params![id])? == 0 {
            return Ok(Err(NotChanged::NoProject));
        }
        let event_id = changed(&tx)?;
        tx.commit()?;
        Ok(Ok(event_id))
    }

    /// Puts a repository in a project, out of the one it was in, or in no
    /// project with `None`. Returns its event's id.
    pub fn set_repo_project(
        &self,
        repo: &str,
        project: Option<&str>,
        at: DateTime<Utc>,
    ) -> rusqlite::Result<Result<i64, NotChanged>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if let Some(project) = project {
            let exists: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?1)",
                params![project],
                |row| row.get(0),
            )?;
            if !exists {
                return Ok(Err(NotChanged::NoProject));
            }
        }
        move_repo(&tx, repo, project, &crate::reviews::iso(at))?;
        let event_id = changed(&tx)?;
        tx.commit()?;
        Ok(Ok(event_id))
    }
}

/// Puts `repo` in `project`, or in none.
fn move_repo(
    conn: &Connection,
    repo: &str,
    project: Option<&str>,
    at: &str,
) -> rusqlite::Result<()> {
    match project {
        Some(project) => conn.execute(
            "INSERT INTO project_repos (repo, project_id, added_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (repo) DO UPDATE SET project_id = excluded.project_id,
               added_at = CASE WHEN project_id = excluded.project_id THEN added_at
                 ELSE excluded.added_at END",
            params![repo, project, at],
        )?,
        None => conn.execute("DELETE FROM project_repos WHERE repo = ?1", params![repo])?,
    };
    Ok(())
}
