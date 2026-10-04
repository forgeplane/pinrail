//! Reviews: writing one, reading one, and the queries the inbox and the
//! history ask, including what a filtered list counts and offers as facets.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value};

use super::Db;
use super::events::insert_event;
use crate::reviews::{Decision, Review, Status, parse_datetime};

#[derive(Debug, Default, Clone)]
pub struct Filters {
    pub statuses: Vec<Status>,
    pub plugin: Option<String>,
    pub repo: Option<String>,
    pub workflow: Option<String>,
    pub reference: Option<String>,
    pub run_id: Option<String>,
    pub text: Option<String>,
    pub include_revised: bool,
    /// Only reviews past this one in the listing's order, for paging.
    pub cursor: Option<String>,
    /// Oldest first, rather than newest first.
    pub oldest_first: bool,
    pub limit: usize,
    /// Rows to skip before the limit, for numbered pages.
    pub offset: usize,
}

/// The values a list's filter menus offer, over the reviews its status
/// filter admits: every plugin and project among them, and whether any has
/// no project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facets {
    pub plugins: Vec<String>,
    pub repos: Vec<String>,
    pub unassigned: bool,
}

/// `repo=-` asks for the reviews that name no project.
pub const NO_PROJECT: &str = "-";

/// Why a review was not stored.
#[derive(Debug, PartialEq, Eq)]
pub enum NotStored {
    /// A pending review is the same submission: its id.
    Twin(String),
    /// A file the review names is no longer stored, removed by the sweep
    /// after the submission was checked: the file's name in the review.
    FileGone(String),
    /// The round this one revises is no longer stored, removed by the
    /// sweep after the submission was checked: its id.
    RevisesGone(String),
}

impl Db {
    /// Moves a pending review from the bundle `from` to `to`, with that
    /// version and request summary, and records the move as an event; `None`
    /// when the review had ended or no longer records `from`, so a move and
    /// a decision made at once have one winner.
    pub fn move_review(
        &self,
        id: &str,
        from: Option<&str>,
        to: &str,
        version: &str,
        summary: Option<&Value>,
        attrs: &Value,
    ) -> rusqlite::Result<Option<i64>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let moved = tx.execute(
            "UPDATE reviews SET plugin_bundle = ?2, plugin_version = ?3, summary = ?4
             WHERE id = ?1 AND plugin_bundle IS ?5
               AND NOT EXISTS (SELECT 1 FROM outcomes WHERE review_id = ?1)
               AND (expires_at IS NULL OR expires_at > ?6)",
            params![
                id,
                to,
                version,
                summary.map(Value::to_string),
                from,
                crate::reviews::iso(chrono::Utc::now())
            ],
        )?;
        if moved == 0 {
            return Ok(None);
        }
        let event_id = insert_event(&tx, Some(id), crate::events::PLUGIN_CHANGED, None, attrs)?;
        tx.commit()?;
        Ok(Some(event_id))
    }

    pub fn insert_review(
        &self,
        review: &Review,
        actor: Option<&str>,
    ) -> rusqlite::Result<Result<i64, NotStored>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let event_id = match insert_in(&tx, review, actor)? {
            Ok(event_id) => event_id,
            Err(not_stored) => return Ok(Err(not_stored)),
        };
        tx.commit()?;
        Ok(Ok(event_id))
    }

    /// Stores the review, unless a pending review is the same submission:
    /// the same plugin, title, payload, origin, round and files. Then it
    /// stores nothing and answers that review's id, as it does for a file
    /// that is no longer stored. Looking and storing
    /// happen under one lock, so two copies sent at once make one review.
    pub fn insert_review_once(
        &self,
        review: &Review,
        actor: Option<&str>,
    ) -> rusqlite::Result<Result<i64, NotStored>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let payload = review
            .payload
            .clone()
            .unwrap_or(Value::Object(Map::new()))
            .to_string();
        let origin = Value::Object(review.origin.clone()).to_string();
        let twins: Vec<String> = tx
            .prepare(
                "SELECT r.id FROM reviews r
                 JOIN review_payloads p ON p.review_id = r.id
                 WHERE r.plugin = ?1 AND r.title = ?2 AND p.payload = ?3 AND r.origin = ?4
                   AND r.revises IS ?5
                   AND NOT EXISTS (SELECT 1 FROM outcomes o WHERE o.review_id = r.id)
                   AND (r.expires_at IS NULL OR r.expires_at > ?6)
                 ORDER BY r.id",
            )?
            .query_map(
                params![
                    review.plugin,
                    review.title,
                    payload,
                    origin,
                    review.revises,
                    crate::reviews::iso(chrono::Utc::now())
                ],
                |row| row.get(0),
            )?
            .collect::<Result<_, _>>()?;
        let mut files: Vec<(String, String)> = review
            .attachments
            .iter()
            .map(|a| (a.name.clone(), a.sha256.clone()))
            .collect();
        files.sort();
        for id in twins {
            let theirs: Vec<(String, String)> = tx
                .prepare(
                    "SELECT name, sha256 FROM review_attachments WHERE review_id = ?1 ORDER BY name, sha256",
                )?
                .query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            if theirs == files {
                return Ok(Err(NotStored::Twin(id)));
            }
        }
        let event_id = match insert_in(&tx, review, actor)? {
            Ok(event_id) => event_id,
            Err(not_stored) => return Ok(Err(not_stored)),
        };
        tx.commit()?;
        Ok(Ok(event_id))
    }
}

/// The review, its created event and its files, in the caller's transaction.
fn insert_in(
    tx: &rusqlite::Transaction<'_>,
    review: &Review,
    actor: Option<&str>,
) -> rusqlite::Result<Result<i64, NotStored>> {
    if let Some(revised) = &review.revises {
        let there: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM reviews WHERE id = ?1)",
            params![revised],
            |row| row.get(0),
        )?;
        if !there {
            return Ok(Err(NotStored::RevisesGone(revised.clone())));
        }
    }
    {
        tx.execute(
            "INSERT INTO reviews (id, plugin, plugin_bundle, plugin_version, title, origin, requested_by, summary, revises, expires_at, created_at, session)
             VALUES (?1, ?2, ?3, ?11, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?12)",
            params![
                review.id,
                review.plugin,
                review.plugin_bundle,
                review.title,
                Value::Object(review.origin.clone()).to_string(),
                review.requested_by,
                review.summary.as_ref().map(|s| s.to_string()),
                review.revises,
                review.expires_at.map(crate::reviews::iso),
                crate::reviews::iso(review.created_at),
                review.plugin_version,
                review.session,
            ],
        )?;
        tx.execute(
            "INSERT INTO review_payloads (review_id, payload) VALUES (?1, ?2)",
            params![
                review.id,
                review
                    .payload
                    .clone()
                    .unwrap_or(Value::Object(Map::new()))
                    .to_string(),
            ],
        )?;
        let event_id = insert_event(
            tx,
            Some(&review.id),
            crate::events::CREATED,
            actor,
            &Value::Null,
        )?;
        for attachment in &review.attachments {
            let inserted = tx.execute(
                "INSERT INTO review_attachments (review_id, name, sha256, size, media_type) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![review.id, attachment.name, attachment.sha256, attachment.size as i64, attachment.media_type],
            );
            // the only foreign key a new review's files can miss is the
            // stored file itself
            match inserted {
                Err(rusqlite::Error::SqliteFailure(error, _))
                    if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY =>
                {
                    return Ok(Err(NotStored::FileGone(attachment.name.clone())));
                }
                inserted => {
                    inserted?;
                }
            }
        }
        Ok(Ok(event_id))
    }
}

impl Db {
    pub fn get_review(&self, id: &str) -> rusqlite::Result<Option<Review>> {
        let conn = self.conn();
        let review = conn
            .query_row(
                &format!("{} WHERE r.id = ?1", select(true)),
                params![id],
                |row| row_to_review(row, true),
            )
            .optional()?;
        let Some(mut review) = review else {
            return Ok(None);
        };
        review.attachments = conn
            .prepare("SELECT name, sha256, size, media_type FROM review_attachments WHERE review_id = ?1 ORDER BY name")?
            .query_map(params![id], |r| {
                Ok(crate::attachments::ReviewAttachment {
                    name: r.get(0)?,
                    sha256: r.get(1)?,
                    size: r.get::<_, i64>(2)? as u64,
                    media_type: r.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(Some(review))
    }

    /// The plugin a review uses, or None when there is no such review.
    pub fn plugin_of(&self, id: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT plugin FROM reviews WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
    }

    /// The round that revises this one, if any.
    pub fn newer_round(&self, id: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT id FROM reviews WHERE revises = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
    }

    pub fn exists(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        conn.query_row("SELECT 1 FROM reviews WHERE id = ?1", params![id], |_| {
            Ok(())
        })
        .optional()
        .map(|r| r.is_some())
    }

    /// Reviews matching the filters, newest first unless the filters ask
    /// for the oldest, without payloads.
    pub fn list(&self, filters: &Filters, now: DateTime<Utc>) -> rusqlite::Result<Vec<Review>> {
        let (mut sql, mut args) = self.where_clause(filters, now);
        let order = if filters.oldest_first { "ASC" } else { "DESC" };
        sql = format!("{}{sql} ORDER BY r.id {order}", select(false));
        if filters.limit > 0 {
            args.push(filters.limit.to_string());
            sql.push_str(&format!(" LIMIT ?{}", args.len()));
            if filters.offset > 0 {
                args.push(filters.offset.to_string());
                sql.push_str(&format!(" OFFSET ?{}", args.len()));
            }
        }
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| row_to_review(row, false))?;
        rows.collect()
    }

    /// How many reviews match the filters; the limit and cursor play no part.
    pub fn count(&self, filters: &Filters, now: DateTime<Utc>) -> rusqlite::Result<usize> {
        let mut filters = filters.clone();
        filters.cursor = None;
        let (sql, args) = self.where_clause(&filters, now);
        let conn = self.conn();
        let params: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id{sql}"
            ),
            params.as_slice(),
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n as usize)
    }

    /// The plugins and projects among the reviews the status filter (and
    /// `include_revised`) admits; the other filters play no part, so a menu
    /// keeps offering what it is filtering by.
    pub fn facets(&self, filters: &Filters, now: DateTime<Utc>) -> rusqlite::Result<Facets> {
        let scope = Filters {
            statuses: filters.statuses.clone(),
            include_revised: filters.include_revised,
            ..Filters::default()
        };
        let (sql, args) = self.where_clause(&scope, now);
        let conn = self.conn();
        let params: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let from = format!("FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id{sql}");
        let strings = |select: &str| -> rusqlite::Result<Vec<String>> {
            let mut stmt = conn.prepare(&format!("{select} {from} ORDER BY 1"))?;
            let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, Option<String>>(0))?;
            Ok(rows
                .filter_map(|r| r.ok().flatten())
                .filter(|v| !v.is_empty())
                .collect())
        };
        let plugins = strings("SELECT DISTINCT r.plugin")?;
        let repos = strings("SELECT DISTINCT json_extract(r.origin, '$.repo')")?;
        let unassigned = conn
            .query_row(
                &format!(
                    "SELECT EXISTS (SELECT 1 {from} AND COALESCE(json_extract(r.origin, '$.repo'), '') = '')"
                ),
                params.as_slice(),
                |row| row.get::<_, bool>(0),
            )?;
        Ok(Facets {
            plugins,
            repos,
            unassigned,
        })
    }

    /// The `WHERE` for the filters, status included, with its parameters.
    fn where_clause(&self, filters: &Filters, now: DateTime<Utc>) -> (String, Vec<String>) {
        let mut sql = " WHERE 1=1".to_string();
        let mut args: Vec<String> = Vec::new();
        let mut push = |value: &str| {
            args.push(value.to_string());
            args.len()
        };
        let no_project = filters.repo.as_deref() == Some(NO_PROJECT);
        if no_project {
            sql.push_str(" AND COALESCE(json_extract(r.origin, '$.repo'), '') = ''");
        }
        let repo = if no_project { &None } else { &filters.repo };
        let exact = [
            ("r.plugin", &filters.plugin),
            ("json_extract(r.origin, '$.repo')", repo),
            ("json_extract(r.origin, '$.workflow')", &filters.workflow),
            ("json_extract(r.origin, '$.ref')", &filters.reference),
            ("json_extract(r.origin, '$.run_id')", &filters.run_id),
        ];
        for (column, value) in exact {
            if let Some(v) = value {
                let n = push(v);
                sql.push_str(&format!(" AND {column} = ?{n}"));
            }
        }
        // Every word somewhere among what a list shows: the title, the
        // plugin, who asked, where it came from, who decided. Not the
        // payload, which can be large and would match almost anything.
        // Case-insensitive, as LIKE is for ASCII.
        if let Some(v) = &filters.text {
            for word in v.split_whitespace() {
                let escaped = word
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                let n = push(&format!("%{escaped}%"));
                let columns = [
                    "r.title",
                    "r.plugin",
                    "r.requested_by",
                    "json_extract(r.origin, '$.repo')",
                    "json_extract(r.origin, '$.workflow')",
                    "json_extract(r.origin, '$.ref')",
                    "o.by",
                ];
                let any: Vec<String> = columns
                    .iter()
                    .map(|c| format!("{c} LIKE ?{n} ESCAPE '\\'"))
                    .collect();
                sql.push_str(&format!(" AND ({})", any.join(" OR ")));
            }
        }
        if let Some(c) = &filters.cursor {
            let n = push(c);
            let past = if filters.oldest_first { ">" } else { "<" };
            sql.push_str(&format!(" AND r.id {past} ?{n}"));
        }
        if !filters.include_revised {
            sql.push_str(" AND NOT EXISTS (SELECT 1 FROM reviews n WHERE n.revises = r.id)");
        }
        if !filters.statuses.is_empty() {
            // only pending and expired read the clock; a bound parameter
            // nothing references is an error to SQLite
            let clocked = filters
                .statuses
                .iter()
                .any(|s| matches!(s, Status::Pending | Status::Expired));
            let n = if clocked {
                push(&crate::reviews::iso(now))
            } else {
                0
            };
            let any: Vec<String> = filters.statuses.iter().map(|s| status_sql(*s, n)).collect();
            sql.push_str(&format!(" AND ({})", any.join(" OR ")));
        }
        (sql, args)
    }

    /// Every round of a review's chain, oldest first, without payloads.
    pub fn rounds(&self, id: &str) -> rusqlite::Result<Vec<Review>> {
        let conn = self.conn();
        let mut root = id.to_string();
        loop {
            let prev: Option<Option<String>> = conn
                .query_row(
                    "SELECT revises FROM reviews WHERE id = ?1",
                    params![root],
                    |row| row.get(0),
                )
                .optional()?;
            match prev {
                Some(Some(p)) => root = p,
                Some(None) => break,
                None => return Ok(Vec::new()),
            }
        }
        let mut out = Vec::new();
        let mut frontier = vec![root];
        while let Some(current) = frontier.pop() {
            let review = conn.query_row(
                &format!("{} WHERE r.id = ?1", select(false)),
                params![current],
                |row| row_to_review(row, false),
            )?;
            out.push(review);
            let mut stmt = conn.prepare("SELECT id FROM reviews WHERE revises = ?1 ORDER BY id")?;
            let next: Vec<String> = stmt
                .query_map(params![current], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            for n in next.into_iter().rev() {
                frontier.push(n);
            }
        }
        Ok(out)
    }

    /// Reviews whose expiry has passed with no decision or withdrawal and no
    /// `expired` event yet.
    pub fn newly_expired(&self, now: DateTime<Utc>) -> rusqlite::Result<Vec<Review>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "{} WHERE r.expires_at IS NOT NULL AND r.expires_at <= ?1
               AND o.review_id IS NULL
               AND NOT EXISTS (SELECT 1 FROM events e WHERE e.review_id = r.id AND e.kind = 'expired')
             ORDER BY r.id",
            select(false)
        ))?;
        let rows = stmt.query_map(params![crate::reviews::iso(now)], |row| {
            row_to_review(row, false)
        })?;
        rows.collect()
    }

    /// Reviews that ended before `before`, as (id, plugin, version): decided,
    /// withdrawn or discarded then, or expired then with nothing recorded.
    /// A round that a round still here revises stays with it, so a chain
    /// goes as a whole and `revises` never dangles.
    pub fn ended_before(
        &self,
        before: DateTime<Utc>,
    ) -> rusqlite::Result<Vec<(String, String, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.plugin, r.plugin_version FROM reviews r
               LEFT JOIN outcomes o ON o.review_id = r.id
              WHERE (o.at IS NOT NULL AND o.at < ?1)
                 OR (o.review_id IS NULL AND r.expires_at IS NOT NULL AND r.expires_at < ?1)
              ORDER BY r.id",
        )?;
        let ended: Vec<(String, String, String)> = stmt
            .query_map(params![crate::reviews::iso(before)], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = conn.prepare("SELECT id, revises FROM reviews WHERE revises IS NOT NULL")?;
        let links: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut going: std::collections::BTreeSet<String> =
            ended.iter().map(|(id, _, _)| id.clone()).collect();
        // a revised round stays while the round revising it stays
        loop {
            let before_len = going.len();
            for (newer, older) in &links {
                if !going.contains(newer) {
                    going.remove(older);
                }
            }
            if going.len() == before_len {
                break;
            }
        }
        Ok(ended
            .into_iter()
            .filter(|(id, _, _)| going.contains(id))
            .collect())
    }

    /// Deletes reviews, all or nothing. Their payloads, events, outcomes
    /// and the record of the files they carried go with them through the
    /// schema's cascades (the blobs themselves go in the attachments sweep),
    /// and a newer round of a deleted review no longer revises it.
    pub fn delete_reviews(&self, ids: &[&str]) -> rusqlite::Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut count = 0;
        for id in ids {
            count += tx.execute("DELETE FROM reviews WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(count)
    }
}

/// The columns `row_to_review` reads. Only a single review joins its
/// payload; a listing selects NULL in its place.
fn select(with_payload: bool) -> String {
    let (payload, join) = if with_payload {
        (
            "p.payload",
            "\n  JOIN review_payloads p ON p.review_id = r.id",
        )
    } else {
        ("NULL", "")
    };
    format!(
        "SELECT r.id, r.plugin, r.plugin_bundle, r.title, r.origin, r.requested_by, {payload}, r.summary,
    r.revises, r.expires_at, r.created_at,
    o.kind, o.at, o.by, o.reason, o.data, o.agent_note,
    r.plugin_version,
    (SELECT count(*) FROM review_attachments a WHERE a.review_id = r.id),
    (SELECT coalesce(sum(a.size), 0) FROM review_attachments a WHERE a.review_id = r.id),
    o.summary,
    r.session
  FROM reviews r
  LEFT JOIN outcomes o ON o.review_id = r.id{join}"
    )
}

/// What `Review::status` says, as SQL over the same columns, so a listing
/// and a count read only the rows they answer with. `?now` is the caller's
/// parameter number for the current time.
fn status_sql(status: Status, now: usize) -> String {
    match status {
        Status::Decided => "o.kind = 'decided'".to_string(),
        Status::Withdrawn => "o.kind = 'withdrawn'".to_string(),
        Status::Discarded => "o.kind = 'discarded'".to_string(),
        Status::Pending => {
            format!("(o.review_id IS NULL AND (r.expires_at IS NULL OR r.expires_at > ?{now}))")
        }
        Status::Expired => {
            format!("(o.review_id IS NULL AND r.expires_at IS NOT NULL AND r.expires_at <= ?{now})")
        }
    }
}

fn row_to_review(row: &rusqlite::Row<'_>, with_payload: bool) -> rusqlite::Result<Review> {
    let origin: String = row.get(4)?;
    let payload: Option<String> = row.get(6)?;
    let summary: Option<String> = row.get(7)?;
    let expires_at: Option<String> = row.get(9)?;
    let created_at: String = row.get(10)?;
    // the outcome, if the review has ended: one row, its kind says which
    // fields it fills
    let kind: Option<String> = row.get(11)?;
    let at: Option<String> = row.get(12)?;
    let by: Option<String> = row.get(13)?;
    let reason: Option<String> = row.get(14)?;
    let data: Option<String> = row.get(15)?;
    let agent_note: Option<String> = row.get(16)?;
    let plugin_version: Option<String> = row.get(17)?;
    let attachments_total = (row.get::<_, i64>(18)? as u64, row.get::<_, i64>(19)? as u64);
    let outcome_summary: Option<String> = row.get(20)?;
    let at = at.and_then(|s| parse_datetime(&s));
    let (
        mut decision,
        mut withdrawn_at,
        mut withdrawn_reason,
        mut discarded_at,
        mut discarded_by,
        mut discarded_reason,
    ) = (None, None, None, None, None, None);
    match kind.as_deref() {
        Some("decided") => {
            decision = Some(Decision {
                decided_by: by.clone().unwrap_or_default(),
                decided_at: at.unwrap_or_default(),
                data: data
                    .as_deref()
                    .and_then(|d| serde_json::from_str(d).ok())
                    .unwrap_or(Value::Null),
                summary: outcome_summary.and_then(|s| serde_json::from_str(&s).ok()),
            })
        }
        Some("withdrawn") => {
            withdrawn_at = at;
            withdrawn_reason = reason.clone();
        }
        Some("discarded") => {
            discarded_at = at;
            discarded_by = by.clone();
            discarded_reason = reason.clone();
        }
        _ => {}
    }
    Ok(Review {
        attachments: Vec::new(),
        attachments_total,
        id: row.get(0)?,
        plugin: row.get(1)?,
        plugin_bundle: row.get(2)?,
        plugin_version: plugin_version.unwrap_or_default(),
        title: row.get(3)?,
        origin: serde_json::from_str::<Value>(&origin)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        requested_by: row.get(5)?,
        session: row.get(21)?,
        payload: payload
            .filter(|_| with_payload)
            .map(|p| serde_json::from_str(&p).unwrap_or(Value::Null)),
        summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
        revises: row.get(8)?,
        expires_at: expires_at.and_then(|s| parse_datetime(&s)),
        created_at: parse_datetime(&created_at).unwrap_or_default(),
        decision,
        agent_note: if kind.as_deref() == Some("decided") {
            agent_note
        } else {
            None
        },
        withdrawn_at,
        withdrawn_reason,
        discarded_at,
        discarded_by,
        discarded_reason,
    })
}
