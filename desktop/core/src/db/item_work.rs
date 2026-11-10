//! Working on items: claims, changes with their rules, comments, and what an
//! item's timeline shows, including the reviews and sessions that belong
//! to it.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};

use super::Db;
use super::items::{
    OPEN, Outcome, Refused, Related, WorkItem, depend_in, key_of, move_in, resolve, row_to_item,
    select, set_parent_in, status_in, touch,
};
use crate::attachments::ReviewAttachment;
use crate::items::{Actor, ItemRef, Status};
use crate::reviews::{iso, parse_datetime};

/// A claim to take or give up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// claim the item; `take` claims it from another session
    Take {
        take: bool,
    },
    Release,
}

/// A change to an item, applied whole or not at all. `None` and empty
/// lists leave a part as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemChange {
    /// whether a parent whose last open sub-item this change closes closes
    /// too, as the person's setting says
    pub close_parents: bool,
    pub title: Option<String>,
    pub body: Option<String>,
    /// `Some(None)` removes the link
    pub link: Option<Option<String>>,
    pub status: Option<Status>,
    pub claim: Option<Claim>,
    /// `Some(None)` takes the item out from under its parent
    pub parent: Option<Option<ItemRef>>,
    pub depends_on: Vec<ItemRef>,
    pub no_depends_on: Vec<ItemRef>,
    /// another repository, by name, to move the item to
    pub repo: Option<String>,
    /// files for the body, already in the blob store
    pub files: Vec<ReviewAttachment>,
    /// files of the body to remove, by name
    pub detach: Vec<String>,
    /// a comment to add with the change
    pub comment: Option<String>,
}

/// An item after a change, with what the change made ready.
#[derive(Debug, Clone, PartialEq)]
pub struct Edited {
    pub item: WorkItem,
    /// items that depended on it, or its parent, ready now that it closed
    pub now_ready: Vec<Related>,
    /// its parent, when it closed the parent's last open sub-item and the
    /// parent stays open: none of its sub-items was done, someone works on
    /// it, it waits on the person, or the setting leaves parents open
    pub parent_left_open: Option<Related>,
    /// the parents, at any depth, that closed with their last sub-item
    pub parents_closed: Vec<Related>,
    /// the sub-items, at any depth, that were cancelled with it, each with
    /// who held it
    pub cancelled_with: Vec<(Related, HeldBy)>,
    /// the sub-items, at any depth, released as it went on hold, each with
    /// who held it
    pub released_with: Vec<(Related, HeldBy)>,
}

/// Who held an item: the agent, and its session when it gave one.
pub type HeldBy = Option<(String, Option<String>)>;

/// A comment on an item.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub id: String,
    pub body: String,
    pub author: String,
    pub session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub files: Vec<ReviewAttachment>,
}

impl Comment {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "body": self.body,
            "author": self.author,
            "session": self.session,
            "created_at": iso(self.created_at),
            "edited_at": self.edited_at.map(iso),
            "files": self.files.iter().map(file_json).collect::<Vec<_>>(),
        })
    }
}

fn file_json(f: &ReviewAttachment) -> Value {
    json!({"name": f.name, "sha256": f.sha256, "size": f.size, "media_type": f.media_type})
}

/// What happened to an item, oldest first: changes, and the comments
/// among them.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Event {
        id: i64,
        kind: String,
        actor: String,
        session: Option<String>,
        at: DateTime<Utc>,
        attrs: Value,
    },
    Comment(Comment),
}

impl Entry {
    pub fn at(&self) -> DateTime<Utc> {
        match self {
            Entry::Event { at, .. } => *at,
            Entry::Comment(c) => c.created_at,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Entry::Event {
                id,
                kind,
                actor,
                session,
                at,
                attrs,
            } => json!({
                "type": "event", "id": id, "kind": kind, "actor": actor, "session": session,
                "at": iso(*at), "attrs": attrs,
            }),
            Entry::Comment(c) => {
                let mut value = c.to_json();
                value["type"] = json!("comment");
                value
            }
        }
    }
}

/// A review or question that belongs to an item, by its newest round.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemReview {
    pub id: String,
    pub title: String,
    pub plugin: String,
    pub kind: String,
    /// pending, decided, withdrawn, discarded or expired
    pub status: String,
    pub summary: Option<Value>,
    pub requested_by: Option<String>,
    /// the agent's session it was sent from, where a question is answered
    pub session: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ItemReview {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "title": self.title, "plugin": self.plugin, "kind": self.kind,
            "status": self.status, "summary": self.summary, "requested_by": self.requested_by,
            "session": self.session, "created_at": iso(self.created_at),
        })
    }
}

/// A session that worked on an item, with its latest status.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemSession {
    pub agent: String,
    pub session: String,
    pub status: Option<String>,
    pub last_at: Option<DateTime<Utc>>,
}

impl ItemSession {
    pub fn to_json(&self) -> Value {
        json!({
            "agent": self.agent, "session": self.session, "status": self.status,
            "last_at": self.last_at.map(iso),
        })
    }
}

/// Records what happened to an item, for its timeline.
pub(super) fn item_event(
    conn: &Connection,
    item: &str,
    kind: &str,
    actor: &Actor,
    attrs: Value,
    at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO work_item_events (item_id, kind, actor, session, at, attrs)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            item,
            kind,
            actor.name(),
            actor.session(),
            iso(at),
            attrs.to_string()
        ],
    )?;
    Ok(())
}

/// The id of the item `id` is part of, if any.
fn parent_id(conn: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT parent FROM work_items WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
}

fn item_row(conn: &Connection, id: &str) -> rusqlite::Result<WorkItem> {
    conn.query_row(
        &format!("{} WHERE i.id = ?1", select()),
        params![id],
        row_to_item,
    )
}

pub(super) fn related(conn: &Connection, id: &str) -> rusqlite::Result<Related> {
    conn.query_row(
        "SELECT i.id, g.prefix || '-' || i.number, i.title,
           CASE WHEN i.held_at IS NOT NULL THEN 'on_hold' ELSE i.status END FROM work_items i
           JOIN repos g ON g.id = i.repo_id WHERE i.id = ?1",
        params![id],
        |r| {
            Ok(Related {
                id: r.get(0)?,
                key: r.get(1)?,
                title: r.get(2)?,
                status: Status::parse(&r.get::<_, String>(3)?).unwrap_or(Status::Open),
            })
        },
    )
}

/// The SQL for where the item `i` is in the queue: its own place, or that
/// of the nearest of its parents that is queued, for a queued parent
/// offers its sub-items in its place. NULL when neither is queued.
const QUEUED_AT: &str = "(WITH RECURSIVE up(id) AS (
     SELECT i.id UNION SELECT a.parent FROM work_items a JOIN up ON a.id = up.id
      WHERE a.parent IS NOT NULL
   ) SELECT MIN(q.position) FROM up JOIN work_item_queue q ON q.item_id = up.id)";

/// Where an item goes in the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    End,
    Before(ItemRef),
    After(ItemRef),
}

/// The key of the item on hold that holds `id` back: itself, or the
/// nearest of its parents that is on hold.
fn held_key(conn: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "WITH RECURSIVE up(id, depth) AS (
           SELECT ?1, 0 UNION SELECT w.parent, up.depth + 1 FROM work_items w JOIN up ON w.id = up.id
            WHERE w.parent IS NOT NULL
         )
         SELECT g.prefix || '-' || a.number FROM up JOIN work_items a ON a.id = up.id
           JOIN repos g ON g.id = a.repo_id
          WHERE a.held_at IS NOT NULL ORDER BY up.depth LIMIT 1",
        params![id],
        |r| r.get(0),
    )
    .optional()
}

/// Whether the holder of the claim, `(agent, session)`, is `actor`.
fn holds(claim: &Option<(String, Option<String>)>, actor: &Actor) -> bool {
    claim.as_ref().is_some_and(|(agent, session)| {
        agent == actor.name() && session.as_deref() == actor.session()
    })
}

/// Claims an item for `actor`: in progress, and held by it.
fn claim_in(
    conn: &Connection,
    id: &str,
    actor: &Actor,
    take: bool,
    at: DateTime<Utc>,
) -> Outcome<()> {
    let item = item_row(conn, id)?;
    if item.status.is_closed() {
        return Ok(Err(Refused::Closed(item.key.to_string(), item.status)));
    }
    // the person takes it off hold by claiming it; an agent is refused
    if matches!(actor, Actor::Agent { .. })
        && let Some(held) = held_key(conn, id)?
    {
        return Ok(Err(Refused::OnHold(held)));
    }
    if holds(&item.claim, actor) {
        return Ok(Ok(()));
    }
    if let Some((agent, session)) = &item.claim
        && !take
    {
        return Ok(Err(Refused::ClaimedBy(
            item.key.to_string(),
            agent.clone(),
            session.clone(),
        )));
    }
    conn.execute(
        "UPDATE work_items SET status = 'in_progress', claim_agent = ?2, claim_session = ?3,
           updated_at = ?4, held_at = NULL WHERE id = ?1",
        params![id, actor.name(), actor.session(), iso(at)],
    )?;
    let from = item
        .claim
        .map(|(agent, session)| json!({"agent": agent, "session": session}));
    item_event(conn, id, "claimed", actor, json!({ "from": from }), at)?;
    Ok(Ok(()))
}

/// Gives up an item's claim: open again. An agent gives up only its own.
fn release_in(conn: &Connection, id: &str, actor: &Actor, at: DateTime<Utc>) -> Outcome<()> {
    let item = item_row(conn, id)?;
    if item.status != Status::InProgress {
        return Ok(Ok(()));
    }
    if matches!(actor, Actor::Agent { .. }) && item.claim.is_some() && !holds(&item.claim, actor) {
        return Ok(Err(Refused::NotYours(format!("the claim of {}", item.key))));
    }
    status_in(conn, id, Status::Open, at)?;
    item_event(conn, id, "released", actor, json!({}), at)?;
    Ok(Ok(()))
}

/// The reviews and questions of an item that wait on the person, newest
/// rounds only, as `"title" (id)`.
fn waiting_on(conn: &Connection, id: &str, now: DateTime<Utc>) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT r.title, r.id FROM reviews r JOIN review_items x ON x.review_id = r.id
           LEFT JOIN outcomes o ON o.review_id = r.id
          WHERE x.item_id = ?1 AND o.review_id IS NULL
            AND (r.expires_at IS NULL OR r.expires_at > ?2)
            AND NOT EXISTS (SELECT 1 FROM reviews n WHERE n.revises = r.id)
          ORDER BY r.id",
    )?;
    stmt.query_map(params![id, iso(now)], |r| {
        Ok(format!(
            "\"{}\" ({})",
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?
        ))
    })?
    .collect()
}

/// The keys of an item's sub-items that are open or in progress.
fn open_sub_items(conn: &Connection, id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT g.prefix || '-' || c.number FROM work_items c JOIN repos g ON g.id = c.repo_id
          WHERE c.parent = ?1 AND c.status IN {OPEN} ORDER BY g.prefix, c.number"
    ))?;
    stmt.query_map(params![id], |r| r.get(0))?.collect()
}

/// Cancels the sub-items of `id` at any depth that are open or in progress,
/// with their claims, each with an entry that says which item took it with
/// it. Done and cancelled ones stay as they are.
fn cancel_sub_items(
    conn: &Connection,
    id: &str,
    actor: &Actor,
    at: DateTime<Utc>,
) -> rusqlite::Result<Vec<(Related, HeldBy)>> {
    let key = key_of(conn, id)?;
    let mut stmt = conn.prepare(&format!(
        "WITH RECURSIVE tree(id) AS (
           SELECT id FROM work_items WHERE parent = ?1
           UNION SELECT w.id FROM work_items w JOIN tree ON w.parent = tree.id
         )
         SELECT w.id, w.status FROM work_items w JOIN tree ON tree.id = w.id
          WHERE w.status IN {OPEN}"
    ))?;
    let open: Vec<(String, String)> = stmt
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let mut cancelled = Vec::new();
    for (sub, from) in open {
        let claim = item_row(conn, &sub)?.claim;
        status_in(conn, &sub, Status::Cancelled, at)?;
        item_event(
            conn,
            &sub,
            "status",
            actor,
            json!({"from": from, "to": "cancelled", "with": key}),
            at,
        )?;
        cancelled.push((related(conn, &sub)?, claim));
    }
    cancelled.sort_by_key(|(r, _)| super::items::sort_key(&r.key));
    Ok(cancelled)
}

/// Releases an item's sub-items in progress, at any depth, as it goes on
/// hold: no one works under an item on hold. Each comes with who held it.
fn release_sub_items(
    conn: &Connection,
    id: &str,
    actor: &Actor,
    at: DateTime<Utc>,
) -> rusqlite::Result<Vec<(Related, HeldBy)>> {
    let key = key_of(conn, id)?;
    let mut stmt = conn.prepare(
        "WITH RECURSIVE tree(id) AS (
           SELECT id FROM work_items WHERE parent = ?1
           UNION SELECT w.id FROM work_items w JOIN tree ON w.parent = tree.id
         )
         SELECT w.id FROM work_items w JOIN tree ON tree.id = w.id
          WHERE w.status = 'in_progress'",
    )?;
    let held: Vec<String> = stmt
        .query_map(params![id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let mut released = Vec::new();
    for sub in held {
        let claim = item_row(conn, &sub)?.claim;
        status_in(conn, &sub, Status::Open, at)?;
        item_event(conn, &sub, "released", actor, json!({ "on_hold": key }), at)?;
        released.push((related(conn, &sub)?, claim));
    }
    released.sort_by_key(|(r, _)| super::items::sort_key(&r.key));
    Ok(released)
}

/// Gives up every claim the session holds, for when it is archived.
pub(super) fn release_session_claims(
    conn: &Connection,
    agent: &str,
    session: &str,
    because: &str,
    at: DateTime<Utc>,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT id FROM work_items WHERE status = 'in_progress'
           AND claim_agent = ?1 AND claim_session = ?2",
    )?;
    let ids: Vec<String> = stmt
        .query_map(params![agent, session], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let actor = Actor::Person;
    for id in &ids {
        status_in(conn, id, Status::Open, at)?;
        item_event(
            conn,
            id,
            "released",
            &actor,
            json!({"because": because, "agent": agent, "session": session}),
            at,
        )?;
    }
    Ok(ids.len())
}

/// Gives the earlier rounds of a new round its items, and records
/// "review_removed" on each item the review left.
pub(super) fn earlier_rounds_follow(
    conn: &Connection,
    round: &str,
    revised: &str,
    actor: &Actor,
    at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    const CHAIN: &str = "WITH RECURSIVE chain(id) AS (
        VALUES (?2) UNION SELECT r.revises FROM reviews r JOIN chain c ON r.id = c.id
         WHERE r.revises IS NOT NULL)";
    let left: Vec<String> = conn
        .prepare(
            "SELECT item_id FROM review_items WHERE review_id = ?2
              AND item_id NOT IN (SELECT item_id FROM review_items WHERE review_id = ?1)",
        )?
        .query_map(params![round, revised], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    conn.execute(
        &format!("{CHAIN} DELETE FROM review_items WHERE review_id IN (SELECT id FROM chain)"),
        params![round, revised],
    )?;
    conn.execute(
        &format!(
            "{CHAIN} INSERT INTO review_items (review_id, item_id)
             SELECT c.id, x.item_id FROM chain c, review_items x WHERE x.review_id = ?1"
        ),
        params![round, revised],
    )?;
    let (title, kind): (String, String) = conn.query_row(
        "SELECT title, kind FROM reviews WHERE id = ?1",
        params![round],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    for id in left {
        item_event(
            conn,
            &id,
            "review_removed",
            actor,
            json!({"review": round, "title": title, "kind": kind}),
            at,
        )?;
        touch(conn, &id, at)?;
    }
    Ok(())
}

/// What a session's agent did to work items, for its thread: `?1` is the
/// agent and `?2` the session.
const SESSION_ITEM_EVENTS: &str = "SELECT e.id, e.kind, e.actor, e.session, e.at, e.attrs,
    w.id, g.prefix || '-' || w.number, w.title
  FROM work_item_events e
  JOIN work_items w ON w.id = e.item_id
  JOIN repos g ON g.id = w.repo_id
 WHERE e.actor = ?1 AND e.session = ?2
   AND e.kind IN ('created', 'claimed', 'released', 'status')
   AND json_extract(e.attrs, '$.with') IS NULL
 ORDER BY e.id";

/// The comments a session's agent wrote on work items, oldest first.
const SESSION_COMMENTS: &str = "SELECT c.id, w.id, g.prefix || '-' || w.number, w.title
  FROM work_item_comments c
  JOIN work_items w ON w.id = c.item_id
  JOIN repos g ON g.id = w.repo_id
 WHERE c.author = ?1 AND c.session = ?2
 ORDER BY c.created_at, c.id";

/// Records on each of its items that a review was sent for it.
pub(super) fn review_sent(conn: &Connection, review: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO work_item_events (item_id, kind, actor, session, at, attrs)
         SELECT x.item_id, 'review_sent', COALESCE(r.requested_by, 'agent'), r.session, r.created_at,
                json_object('review', r.id, 'title', r.title, 'plugin', r.plugin, 'kind', r.kind,
                            'revises', r.revises)
           FROM reviews r JOIN review_items x ON x.review_id = r.id WHERE r.id = ?1",
        params![review],
    )?;
    Ok(())
}

/// Records on each of its items how a review ended, with the decision's summary or
/// the reason, so that the item keeps it after the review is swept.
pub(super) fn review_ended(
    conn: &Connection,
    review: &str,
    outcome: &str,
    by: Option<&str>,
    detail: &Value,
    at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO work_item_events (item_id, kind, actor, session, at, attrs)
         SELECT x.item_id, 'review_ended', COALESCE(?3, 'person'), NULL, ?4,
                json_object('review', r.id, 'title', r.title, 'plugin', r.plugin, 'kind', r.kind,
                            'outcome', ?2, 'detail', json(?5))
           FROM reviews r JOIN review_items x ON x.review_id = r.id WHERE r.id = ?1",
        params![review, outcome, by, iso(at), detail.to_string()],
    )?;
    Ok(())
}

fn comment_row(conn: &Connection, id: &str) -> rusqlite::Result<Option<Comment>> {
    let comment = conn
        .query_row(
            "SELECT id, body, author, session, created_at, edited_at
               FROM work_item_comments WHERE id = ?1",
            params![id],
            |r| {
                Ok(Comment {
                    id: r.get(0)?,
                    body: r.get(1)?,
                    author: r.get(2)?,
                    session: r.get(3)?,
                    created_at: parse_datetime(&r.get::<_, String>(4)?).unwrap_or_default(),
                    edited_at: r
                        .get::<_, Option<String>>(5)?
                        .and_then(|t| parse_datetime(&t)),
                    files: Vec::new(),
                })
            },
        )
        .optional()?;
    let Some(mut comment) = comment else {
        return Ok(None);
    };
    comment.files = files(conn, "comment_id = ?1", &comment.id)?;
    Ok(Some(comment))
}

fn files(conn: &Connection, filter: &str, arg: &str) -> rusqlite::Result<Vec<ReviewAttachment>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT name, sha256, size, media_type FROM work_item_files WHERE {filter} ORDER BY name"
    ))?;
    stmt.query_map(params![arg], |r| {
        Ok(ReviewAttachment {
            name: r.get(0)?,
            sha256: r.get(1)?,
            size: r.get::<_, i64>(2)? as u64,
            media_type: r.get(3)?,
        })
    })?
    .collect()
}

/// Adds files to the item's description, or to a comment. A file replaces
/// one of the same name that belongs to the same description or comment;
/// a name another comment or the description already uses is refused,
/// since the file would replace that one.
fn add_files(
    conn: &Connection,
    item: &str,
    comment: Option<&str>,
    added: &[ReviewAttachment],
) -> rusqlite::Result<Result<(), Refused>> {
    for f in added {
        let owner: Option<Option<String>> = conn
            .query_row(
                "SELECT comment_id FROM work_item_files WHERE item_id = ?1 AND name = ?2",
                params![item, f.name],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(owner) = owner
            && owner.as_deref() != comment
        {
            return Ok(Err(Refused::FileTaken(
                key_of(conn, item)?,
                f.name.clone(),
                owner.is_none(),
            )));
        }
    }
    for f in added {
        conn.execute(
            "INSERT INTO work_item_files (item_id, comment_id, name, sha256, size, media_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (item_id, name) DO UPDATE SET comment_id = excluded.comment_id,
               sha256 = excluded.sha256, size = excluded.size, media_type = excluded.media_type",
            params![item, comment, f.name, f.sha256, f.size as i64, f.media_type],
        )?;
    }
    Ok(Ok(()))
}

impl Db {
    /// The next ready item of a repository, as `items next` chooses it: the
    /// sub-items of what the session holds first, then the item that
    /// unblocks the most others, then the oldest. With `claim`, it is
    /// claimed for `actor` in the same transaction, so that two sessions
    /// asking at once get two items.
    pub fn next_item(
        &self,
        repo: &str,
        actor: &Actor,
        claim: bool,
        at: DateTime<Utc>,
    ) -> rusqlite::Result<Option<WorkItem>> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: Option<String> = tx
            .query_row(
                &format!(
                    "WITH RECURSIVE mine(id) AS (
                       SELECT id FROM work_items WHERE status = 'in_progress'
                          AND claim_agent = ?2 AND claim_session IS ?3
                       UNION SELECT w.id FROM work_items w JOIN mine ON w.parent = mine.id
                     )
                     SELECT i.id FROM work_items i JOIN repos g ON g.id = i.repo_id
                      WHERE g.name = ?1 AND {ready}
                      ORDER BY i.id IN mine DESC,
                        {queued} IS NULL, {queued},
                        (SELECT COUNT(*) FROM work_item_deps x JOIN work_items d ON d.id = x.item_id
                          WHERE x.depends_on = i.id AND d.status IN {OPEN}) DESC,
                        i.number
                      LIMIT 1",
                    ready = super::items::READY,
                    queued = QUEUED_AT,
                ),
                params![repo, actor.name(), actor.session()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else {
            return Ok(None);
        };
        if claim {
            claim_in(&tx, &id, actor, false, at)?
                .expect("a ready item is open, so it can be claimed");
        }
        let item = item_row(&tx, &id)?;
        tx.commit()?;
        Ok(Some(item))
    }

    /// Changes an item as `change` says, whole or not at all, recording each
    /// part on its timeline. No one marks an item done while a sub-item is
    /// open, and an agent only when nothing of it waits on the person.
    pub fn edit_item(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        change: &ItemChange,
        actor: &Actor,
        at: DateTime<Utc>,
    ) -> Outcome<Edited> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, repo)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let before = item_row(&tx, &id)?;
        // the item's own repository, for numbers alone in the change
        let here = before.repo.clone();
        let find = |r: &ItemRef| resolve(&tx, r, Some(&here));

        let mut edited = Vec::new();
        if let Some(title) = &change.title
            && *title != before.title
        {
            tx.execute(
                "UPDATE work_items SET title = ?2 WHERE id = ?1",
                params![id, title],
            )?;
            edited.push("title");
        }
        if let Some(body) = &change.body
            && *body != before.body
        {
            tx.execute(
                "UPDATE work_items SET body = ?2 WHERE id = ?1",
                params![id, body],
            )?;
            edited.push("body");
        }
        if let Some(link) = &change.link
            && *link != before.link
        {
            tx.execute(
                "UPDATE work_items SET link = ?2 WHERE id = ?1",
                params![id, link],
            )?;
            edited.push("link");
        }
        if !edited.is_empty() {
            item_event(&tx, &id, "edited", actor, json!({ "fields": edited }), at)?;
        }
        if !change.files.is_empty() || !change.detach.is_empty() {
            for name in &change.detach {
                tx.execute(
                    "DELETE FROM work_item_files WHERE item_id = ?1 AND name = ?2 AND comment_id IS NULL",
                    params![id, name],
                )?;
            }
            if let Err(refused) = add_files(&tx, &id, None, &change.files)? {
                return Ok(Err(refused));
            }
            let added: Vec<&str> = change.files.iter().map(|f| f.name.as_str()).collect();
            item_event(
                &tx,
                &id,
                "files",
                actor,
                json!({"added": added, "removed": change.detach}),
                at,
            )?;
        }
        if let Some(parent) = &change.parent {
            let parent = match parent {
                Some(p) => match find(p)? {
                    Ok(p) => Some(p),
                    Err(refused) => return Ok(Err(refused)),
                },
                None => None,
            };
            if let Err(refused) = set_parent_in(&tx, &id, parent.as_deref())? {
                return Ok(Err(refused));
            }
            let to = parent.as_deref().map(|p| key_of(&tx, p)).transpose()?;
            item_event(&tx, &id, "parent", actor, json!({ "to": to }), at)?;
        }
        let mut added = Vec::new();
        let mut removed = Vec::new();
        for other in &change.no_depends_on {
            let other = match find(other)? {
                Ok(o) => o,
                Err(refused) => return Ok(Err(refused)),
            };
            if tx.execute(
                "DELETE FROM work_item_deps WHERE item_id = ?1 AND depends_on = ?2",
                params![id, other],
            )? > 0
            {
                removed.push(key_of(&tx, &other)?);
            }
        }
        for other in &change.depends_on {
            let other = match find(other)? {
                Ok(o) => o,
                Err(refused) => return Ok(Err(refused)),
            };
            if let Err(refused) = depend_in(&tx, &id, &other)? {
                return Ok(Err(refused));
            }
            added.push(key_of(&tx, &other)?);
        }
        if !added.is_empty() || !removed.is_empty() {
            item_event(
                &tx,
                &id,
                "depends",
                actor,
                json!({"added": added, "removed": removed}),
                at,
            )?;
        }
        if let Some(to) = &change.repo {
            let from = before.key.to_string();
            if move_in(&tx, &id, to, at)? {
                let now = key_of(&tx, &id)?;
                item_event(
                    &tx,
                    &id,
                    "moved",
                    actor,
                    json!({"from": from, "to": now}),
                    at,
                )?;
            }
        }
        match change.claim {
            Some(Claim::Take { take }) => {
                if let Err(refused) = claim_in(&tx, &id, actor, take, at)? {
                    return Ok(Err(refused));
                }
            }
            Some(Claim::Release) => {
                if let Err(refused) = release_in(&tx, &id, actor, at)? {
                    return Ok(Err(refused));
                }
            }
            None => {}
        }
        let mut closed_now = false;
        let mut cancelled_with = Vec::new();
        let mut released_with = Vec::new();
        if let Some(status) = change.status {
            let current = item_row(&tx, &id)?;
            // the person decides what waits: an agent neither puts an item
            // on hold nor takes it off
            if matches!(actor, Actor::Agent { .. })
                && status != current.status
                && (status == Status::OnHold || current.status == Status::OnHold)
            {
                return Ok(Err(Refused::OnHold(current.key.to_string())));
            }
            // an agent closes only what it holds, or what no one holds: it
            // takes another session's claim first, in the open
            if matches!(actor, Actor::Agent { .. })
                && status.is_closed()
                && current.status == Status::InProgress
                && !holds(&current.claim, actor)
                && let Some((agent, session)) = &current.claim
            {
                return Ok(Err(Refused::ClaimedBy(
                    current.key.to_string(),
                    agent.clone(),
                    session.clone(),
                )));
            }
            // a sub-item opened again needs its parent open
            if current.status.is_closed()
                && !status.is_closed()
                && let Some(parent) = parent_id(&tx, &id)?
                && let Some(refused) = super::items::closed(&tx, &parent)?
            {
                return Ok(Err(refused));
            }
            if status != current.status {
                match status {
                    Status::InProgress => {
                        if let Err(refused) = claim_in(&tx, &id, actor, false, at)? {
                            return Ok(Err(refused));
                        }
                    }
                    Status::Open if current.status == Status::InProgress => {
                        if let Err(refused) = release_in(&tx, &id, actor, at)? {
                            return Ok(Err(refused));
                        }
                    }
                    _ => {
                        // no one marks an item done while a sub-item is
                        // open; an agent waits for the person's answers too,
                        // which the person can give at any time
                        if status == Status::Done {
                            let waiting = if matches!(actor, Actor::Agent { .. }) {
                                waiting_on(&tx, &id, at)?
                            } else {
                                Vec::new()
                            };
                            let open = open_sub_items(&tx, &id)?;
                            if !waiting.is_empty() || !open.is_empty() {
                                return Ok(Err(Refused::NotDone {
                                    key: current.key.to_string(),
                                    waiting,
                                    open,
                                }));
                            }
                        }
                        status_in(&tx, &id, status, at)?;
                        item_event(
                            &tx,
                            &id,
                            "status",
                            actor,
                            json!({"from": current.status.as_str(), "to": status.as_str()}),
                            at,
                        )?;
                        closed_now = status.is_closed() && !current.status.is_closed();
                        if status == Status::Cancelled {
                            cancelled_with = cancel_sub_items(&tx, &id, actor, at)?;
                        }
                        if status == Status::OnHold {
                            released_with = release_sub_items(&tx, &id, actor, at)?;
                        }
                    }
                }
            }
        }
        if let Some(text) = &change.comment {
            insert_comment(&tx, &id, text, actor, &[], at)?
                .expect("a comment without files is not refused");
        }
        touch(&tx, &id, at)?;

        // what its closing changed for the items that waited for it
        let mut now_ready = Vec::new();
        let mut parent_left_open = None;
        let mut parents_closed = Vec::new();
        if closed_now {
            let mut stmt =
                tx.prepare("SELECT item_id FROM work_item_deps WHERE depends_on = ?1")?;
            let dependents: Vec<String> = stmt
                .query_map(params![id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            drop(stmt);
            for other in dependents {
                if item_row(&tx, &other)?.ready {
                    now_ready.push(related(&tx, &other)?);
                }
            }
            // a parent whose last open sub-item closed closes too, and so on
            // up, unless someone works on it, it is on hold, something of it
            // waits on the person, or the setting leaves parents open
            let mut child = id.clone();
            loop {
                let parent: Option<String> = tx.query_row(
                    "SELECT parent FROM work_items WHERE id = ?1",
                    params![child],
                    |r| r.get(0),
                )?;
                let Some(parent) = parent else { break };
                let row = item_row(&tx, &parent)?;
                if row.status.is_closed() || !open_sub_items(&tx, &parent)?.is_empty() {
                    break;
                }
                // done when some of its work was done: a parent whose
                // sub-items were all cancelled has its own work left
                let some_done: bool = tx.query_row(
                    "SELECT EXISTS (SELECT 1 FROM work_items WHERE parent = ?1 AND status = 'done')",
                    params![parent],
                    |r| r.get(0),
                )?;
                if !change.close_parents
                    || !some_done
                    || row.status != Status::Open
                    || !waiting_on(&tx, &parent, at)?.is_empty()
                {
                    parent_left_open = Some(related(&tx, &parent)?);
                    break;
                }
                status_in(&tx, &parent, Status::Done, at)?;
                item_event(
                    &tx,
                    &parent,
                    "status",
                    actor,
                    json!({"from": "open", "to": "done", "with_last": key_of(&tx, &child)?}),
                    at,
                )?;
                parents_closed.push(related(&tx, &parent)?);
                child = parent;
            }
        }
        let item = item_row(&tx, &id)?;
        tx.commit()?;
        Ok(Ok(Edited {
            item,
            now_ready,
            parent_left_open,
            parents_closed,
            cancelled_with,
            released_with,
        }))
    }

    /// The sessions holding a claim of an item, for an agent's request
    /// that names no item: the items `agent` holds in `session`.
    pub fn session_claims(&self, agent: &str, session: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id FROM work_items WHERE status = 'in_progress'
               AND claim_agent = ?1 AND claim_session = ?2 ORDER BY id",
        )?;
        stmt.query_map(params![agent, session], |r| r.get(0))?
            .collect()
    }

    /// Adds a comment, with its files, already in the blob store.
    pub fn add_item_comment(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        body: &str,
        files: &[ReviewAttachment],
        actor: &Actor,
        at: DateTime<Utc>,
    ) -> Outcome<(WorkItem, Comment)> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, repo)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let comment = match insert_comment(&tx, &id, body, actor, files, at)? {
            Ok(comment) => comment,
            Err(refused) => return Ok(Err(refused)),
        };
        touch(&tx, &id, at)?;
        let item = item_row(&tx, &id)?;
        tx.commit()?;
        Ok(Ok((item, comment)))
    }

    /// Changes a comment's text: its author alone can.
    pub fn edit_item_comment(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        comment: &str,
        body: &str,
        actor: &Actor,
        at: DateTime<Utc>,
    ) -> Outcome<Comment> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, repo)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let author: Option<String> = tx
            .query_row(
                "SELECT author FROM work_item_comments WHERE id = ?1 AND item_id = ?2",
                params![comment, id],
                |r| r.get(0),
            )
            .optional()?;
        match author {
            None => return Ok(Err(Refused::NoComment(comment.to_string()))),
            Some(author) if author != actor.name() => {
                return Ok(Err(Refused::NotYours(format!("comment {comment}"))));
            }
            Some(_) => {}
        }
        tx.execute(
            "UPDATE work_item_comments SET body = ?2, edited_at = ?3 WHERE id = ?1",
            params![comment, body, iso(at)],
        )?;
        let updated = comment_row(&tx, comment)?.expect("the comment was just found");
        tx.commit()?;
        Ok(Ok(updated))
    }

    /// Deletes a comment, with its files: its author can, and the person
    /// can delete any.
    pub fn delete_item_comment(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        comment: &str,
        actor: &Actor,
    ) -> Outcome<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, repo)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let author: Option<String> = tx
            .query_row(
                "SELECT author FROM work_item_comments WHERE id = ?1 AND item_id = ?2",
                params![comment, id],
                |r| r.get(0),
            )
            .optional()?;
        match author {
            None => return Ok(Err(Refused::NoComment(comment.to_string()))),
            Some(author) if *actor != Actor::Person && author != actor.name() => {
                return Ok(Err(Refused::NotYours(format!("comment {comment}"))));
            }
            Some(_) => {}
        }
        tx.execute(
            "DELETE FROM work_item_comments WHERE id = ?1",
            params![comment],
        )?;
        tx.commit()?;
        Ok(Ok(()))
    }

    /// An item's timeline, oldest first: its changes and its comments.
    pub fn item_timeline(&self, id: &str) -> rusqlite::Result<Vec<Entry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, kind, actor, session, at, attrs FROM work_item_events
              WHERE item_id = ?1 ORDER BY id",
        )?;
        let mut entries: Vec<Entry> = stmt
            .query_map(params![id], |r| {
                Ok(Entry::Event {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    actor: r.get(2)?,
                    session: r.get(3)?,
                    at: parse_datetime(&r.get::<_, String>(4)?).unwrap_or_default(),
                    attrs: r
                        .get::<_, Option<String>>(5)?
                        .and_then(|a| serde_json::from_str(&a).ok())
                        .unwrap_or(Value::Null),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM work_item_comments WHERE item_id = ?1 ORDER BY created_at, id",
        )?;
        let ids: Vec<String> = stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for comment in ids {
            if let Some(c) = comment_row(&conn, &comment)? {
                entries.push(Entry::Comment(c));
            }
        }
        // in order of time; a stable sort keeps the events' own order
        entries.sort_by_key(Entry::at);
        Ok(entries)
    }

    /// What an agent's session did to work items, oldest first, for its
    /// thread: the items it added, claimed and released, and the statuses
    /// it gave them, without the sub-items a cancel took along.
    pub fn session_item_events(
        &self,
        agent: &str,
        session: &str,
    ) -> rusqlite::Result<Vec<(Entry, super::ItemTag)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(SESSION_ITEM_EVENTS)?;
        stmt.query_map(params![agent, session], |r| {
            Ok((
                Entry::Event {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    actor: r.get(2)?,
                    session: r.get(3)?,
                    at: parse_datetime(&r.get::<_, String>(4)?).unwrap_or_default(),
                    attrs: r
                        .get::<_, Option<String>>(5)?
                        .and_then(|a| serde_json::from_str(&a).ok())
                        .unwrap_or(Value::Null),
                },
                super::ItemTag {
                    id: r.get(6)?,
                    key: r.get(7)?,
                    title: r.get(8)?,
                },
            ))
        })?
        .collect()
    }

    /// The comments an agent's session wrote on work items, oldest first,
    /// each with its item, for the session's thread.
    pub fn session_comments(
        &self,
        agent: &str,
        session: &str,
    ) -> rusqlite::Result<Vec<(Comment, super::ItemTag)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(SESSION_COMMENTS)?;
        let rows = stmt
            .query_map(params![agent, session], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    super::ItemTag {
                        id: r.get(1)?,
                        key: r.get(2)?,
                        title: r.get(3)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for (id, item) in rows {
            if let Some(comment) = comment_row(&conn, &id)? {
                out.push((comment, item));
            }
        }
        Ok(out)
    }

    /// An item's comments, oldest first.
    pub fn item_comments(&self, id: &str) -> rusqlite::Result<Vec<Comment>> {
        Ok(self
            .item_timeline(id)?
            .into_iter()
            .filter_map(|e| match e {
                Entry::Comment(c) => Some(c),
                Entry::Event { .. } => None,
            })
            .collect())
    }

    /// The reviews and questions of an item, by their newest rounds,
    /// oldest first.
    pub fn item_reviews(&self, id: &str, now: DateTime<Utc>) -> rusqlite::Result<Vec<ItemReview>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.title, r.plugin, r.kind,
               CASE WHEN o.kind IS NOT NULL THEN o.kind
                    WHEN r.expires_at IS NOT NULL AND r.expires_at <= ?2 THEN 'expired'
                    ELSE 'pending' END,
               o.summary, r.requested_by, r.created_at, r.session
             FROM reviews r JOIN review_items x ON x.review_id = r.id
             LEFT JOIN outcomes o ON o.review_id = r.id
            WHERE x.item_id = ?1 AND NOT EXISTS (SELECT 1 FROM reviews n WHERE n.revises = r.id)
            ORDER BY r.id",
        )?;
        stmt.query_map(params![id, iso(now)], |r| {
            Ok(ItemReview {
                id: r.get(0)?,
                title: r.get(1)?,
                plugin: r.get(2)?,
                kind: r.get(3)?,
                status: r.get(4)?,
                summary: r
                    .get::<_, Option<String>>(5)?
                    .and_then(|s| serde_json::from_str(&s).ok()),
                requested_by: r.get(6)?,
                session: r.get(8)?,
                created_at: parse_datetime(&r.get::<_, String>(7)?).unwrap_or_default(),
            })
        })?
        .collect()
    }

    /// The sessions that worked on an item: that claimed it, changed it,
    /// commented on it or sent a review for it, with each one's latest
    /// status.
    pub fn item_sessions(&self, id: &str) -> rusqlite::Result<Vec<ItemSession>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "WITH worked(agent, session) AS (
               SELECT actor, session FROM work_item_events WHERE item_id = ?1 AND session IS NOT NULL
               UNION SELECT author, session FROM work_item_comments WHERE item_id = ?1 AND session IS NOT NULL
               UNION SELECT r.requested_by, r.session FROM reviews r
                      JOIN review_items x ON x.review_id = r.id
                      WHERE x.item_id = ?1 AND r.requested_by IS NOT NULL AND r.session IS NOT NULL
               UNION SELECT created_by, created_session FROM work_items
                      WHERE id = ?1 AND created_session IS NOT NULL
             )
             SELECT w.agent, w.session,
               (SELECT t.text FROM session_statuses t WHERE t.agent = w.agent AND t.session = w.session
                  AND t.kind <> 'note' ORDER BY t.id DESC LIMIT 1),
               s.last_at
             FROM worked w LEFT JOIN sessions s ON s.agent = w.agent AND s.id = w.session
             ORDER BY s.last_at DESC, w.agent, w.session",
        )?;
        stmt.query_map(params![id], |r| {
            Ok(ItemSession {
                agent: r.get(0)?,
                session: r.get(1)?,
                status: r.get(2)?,
                last_at: r
                    .get::<_, Option<String>>(3)?
                    .and_then(|t| parse_datetime(&t)),
            })
        })?
        .collect()
    }

    /// The files of an item's body.
    pub fn item_body_files(&self, id: &str) -> rusqlite::Result<Vec<ReviewAttachment>> {
        files(&self.conn(), "item_id = ?1 AND comment_id IS NULL", id)
    }

    /// One of an item's files, of its body or of a comment, by name.
    pub fn item_file(&self, id: &str, name: &str) -> rusqlite::Result<Option<ReviewAttachment>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT name, sha256, size, media_type FROM work_item_files WHERE item_id = ?1 AND name = ?2",
            params![id, name],
            |r| {
                Ok(ReviewAttachment {
                    name: r.get(0)?,
                    sha256: r.get(1)?,
                    size: r.get::<_, i64>(2)? as u64,
                    media_type: r.get(3)?,
                })
            },
        )
        .optional()
    }

    /// Records on its item that a review expired before anyone decided.
    pub fn item_review_expired(&self, review: &str, at: DateTime<Utc>) -> rusqlite::Result<()> {
        review_ended(&self.conn(), review, "expired", None, &Value::Null, at)
    }

    /// The ids of the items a review belongs to.
    pub fn items_of_review(&self, review: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT item_id FROM review_items WHERE review_id = ?1 ORDER BY item_id")?;
        stmt.query_map(params![review], |r| r.get(0))?.collect()
    }

    /// Puts every round of a review under exactly `items`, none for an
    /// empty list. Each item the review joins records "review_added" and
    /// each it leaves "review_removed". `rounds` are the review's rounds,
    /// oldest first. Returns the ids of the items it joined or left.
    pub fn set_review_items(
        &self,
        rounds: &[String],
        items: &[ItemRef],
        actor: &Actor,
        at: DateTime<Utc>,
    ) -> Outcome<Vec<String>> {
        let Some(newest) = rounds.last() else {
            return Ok(Ok(Vec::new()));
        };
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut ids = Vec::new();
        for item in items {
            match resolve(&tx, item, None)? {
                Ok(id) if !ids.contains(&id) => ids.push(id),
                Ok(_) => {}
                Err(refused) => return Ok(Err(refused)),
            }
        }
        let before: Vec<String> = tx
            .prepare("SELECT item_id FROM review_items WHERE review_id = ?1")?
            .query_map(params![newest], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for round in rounds {
            tx.execute(
                "DELETE FROM review_items WHERE review_id = ?1",
                params![round],
            )?;
            for id in &ids {
                tx.execute(
                    "INSERT INTO review_items (review_id, item_id) VALUES (?1, ?2)",
                    params![round, id],
                )?;
            }
        }
        let (title, kind): (String, String) = tx.query_row(
            "SELECT title, kind FROM reviews WHERE id = ?1",
            params![newest],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let attrs = json!({"review": newest, "title": title, "kind": kind});
        let mut changed = Vec::new();
        for id in before.iter().filter(|id| !ids.contains(id)) {
            item_event(&tx, id, "review_removed", actor, attrs.clone(), at)?;
            changed.push(id.clone());
        }
        for id in ids.iter().filter(|id| !before.contains(id)) {
            item_event(&tx, id, "review_added", actor, attrs.clone(), at)?;
            changed.push(id.clone());
        }
        for id in &changed {
            touch(&tx, id, at)?;
        }
        tx.commit()?;
        Ok(Ok(changed))
    }

    /// Puts an item in the person's queue, or moves it there: at the end,
    /// or before or after a queued item. An item on hold comes off hold;
    /// a closed one is refused.
    pub fn queue_item(&self, item: &ItemRef, place: &Place, at: DateTime<Utc>) -> Outcome<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, None)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let current = item_row(&tx, &id)?;
        if current.status.is_closed() {
            return Ok(Err(Refused::Closed(
                current.key.to_string(),
                current.status,
            )));
        }
        let person = Actor::Person;
        if current.status == Status::OnHold {
            status_in(&tx, &id, Status::Open, at)?;
            item_event(
                &tx,
                &id,
                "status",
                &person,
                json!({"from": "on_hold", "to": "open"}),
                at,
            )?;
        }
        // the others, numbered 1, 2, 3… in their order, so that there is
        // always room between two of them
        let others: Vec<String> = tx
            .prepare("SELECT item_id FROM work_item_queue WHERE item_id <> ?1 ORDER BY position")?
            .query_map(params![id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for (n, other) in others.iter().enumerate() {
            tx.execute(
                "UPDATE work_item_queue SET position = ?2 WHERE item_id = ?1",
                params![other, (n + 1) as f64],
            )?;
        }
        let at_of = |key: &ItemRef| -> rusqlite::Result<Result<usize, Refused>> {
            let other = match resolve(&tx, key, None)? {
                Ok(other) => other,
                Err(refused) => return Ok(Err(refused)),
            };
            if other == id {
                return Ok(Err(Refused::OwnPlace(key_of(&tx, &other)?)));
            }
            match others.iter().position(|o| *o == other) {
                Some(n) => Ok(Ok(n + 1)),
                None => Ok(Err(Refused::NotQueued(key_of(&tx, &other)?))),
            }
        };
        let position = match place {
            Place::End => others.len() as f64 + 1.0,
            Place::Before(key) => match at_of(key)? {
                Ok(n) => n as f64 - 0.5,
                Err(refused) => return Ok(Err(refused)),
            },
            Place::After(key) => match at_of(key)? {
                Ok(n) => n as f64 + 0.5,
                Err(refused) => return Ok(Err(refused)),
            },
        };
        let was = tx
            .query_row(
                "SELECT 1 FROM work_item_queue WHERE item_id = ?1",
                params![id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        tx.execute(
            "INSERT INTO work_item_queue (item_id, position) VALUES (?1, ?2)
             ON CONFLICT (item_id) DO UPDATE SET position = excluded.position",
            params![id, position],
        )?;
        if !was {
            item_event(&tx, &id, "queued", &person, json!({}), at)?;
        }
        touch(&tx, &id, at)?;
        tx.commit()?;
        Ok(Ok(()))
    }

    /// Takes an item out of the person's queue.
    pub fn unqueue_item(&self, item: &ItemRef, at: DateTime<Utc>) -> Outcome<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, None)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let removed = tx.execute(
            "DELETE FROM work_item_queue WHERE item_id = ?1",
            params![id],
        )?;
        if removed > 0 {
            item_event(&tx, &id, "unqueued", &Actor::Person, json!({}), at)?;
            touch(&tx, &id, at)?;
        }
        tx.commit()?;
        Ok(Ok(()))
    }
}

fn insert_comment(
    conn: &Connection,
    item: &str,
    body: &str,
    actor: &Actor,
    files: &[ReviewAttachment],
    at: DateTime<Utc>,
) -> rusqlite::Result<Result<Comment, Refused>> {
    let id = crate::id::next_comment();
    conn.execute(
        "INSERT INTO work_item_comments (id, item_id, body, author, session, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, item, body, actor.name(), actor.session(), iso(at)],
    )?;
    if let Err(refused) = add_files(conn, item, Some(&id), files)? {
        return Ok(Err(refused));
    }
    Ok(Ok(
        comment_row(conn, &id)?.expect("the comment was just added")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steps of a query's plan.
    fn plan(db: &Db, sql: &str) -> Vec<String> {
        let conn = db.conn();
        conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map(params!["claude-code", "s-1"], |r| r.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn a_sessions_item_lines_and_comments_are_read_through_their_indexes() {
        let db = Db::in_memory().unwrap();
        let steps = plan(&db, SESSION_ITEM_EVENTS);
        assert!(
            steps.iter().any(|s| s.contains("work_item_events_session")),
            "{steps:#?}"
        );
        let steps = plan(&db, SESSION_COMMENTS);
        assert!(
            steps
                .iter()
                .any(|s| s.contains("work_item_comments_session")),
            "{steps:#?}"
        );
        assert!(
            !steps.iter().any(|s| s.contains("TEMP B-TREE")),
            "{steps:#?}"
        );
    }
}
