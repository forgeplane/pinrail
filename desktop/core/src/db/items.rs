//! Work items as stored: each in a repository that numbers it, under an
//! optional parent, depending on other items.
//!
//! An item waits for the items it depends on, and a parent waits for its
//! sub-items. Together these form one graph that must have no cycle: a cycle
//! of dependencies, a parent under its own sub-item, or a sub-item that
//! depends on its parent would each leave items that can never be ready.
//! Every change to a parent or a dependency is checked against that graph.

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params, params_from_iter};
use serde_json::{Value, json};

use super::Db;
use crate::items::keys::{self, Key};
use crate::items::{Actor, Breakdown, ItemRef, NewItem, Status};
use crate::reviews::{iso, parse_datetime};

/// An item as the lists and the detail show it, with what is derived from
/// its relations.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkItem {
    pub id: String,
    pub key: Key,
    pub repo: String,
    pub title: String,
    pub body: String,
    pub status: Status,
    /// the parent's key, if it has one
    pub parent: Option<String>,
    pub link: Option<String>,
    /// the agent and session holding the claim, while in progress
    pub claim: Option<(String, Option<String>)>,
    pub created_by: String,
    pub created_session: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    /// how many sub-items it has, and how many of them are closed
    pub sub_items: usize,
    pub sub_items_closed: usize,
    /// its own sub-items that a session works on now
    pub sub_items_in_progress: usize,
    /// the keys of the items it depends on that are still open or in
    /// progress, sorted
    pub blocked_by: Vec<String>,
    /// open, not blocked, and with no open sub-item
    pub ready: bool,
    /// the project its repository is in
    pub project: Option<super::ProjectRef>,
    /// how many of its reviews and questions wait on the person
    pub waiting: usize,
    /// its place in the person's queue, from 1, if it is queued
    pub queue: Option<usize>,
}

impl WorkItem {
    pub fn blocked(&self) -> bool {
        !self.blocked_by.is_empty()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "key": self.key.to_string(),
            "number": self.key.number,
            "repo": self.repo,
            "title": self.title,
            "body": self.body,
            "status": self.status.as_str(),
            "parent": self.parent,
            "link": self.link,
            "claim": self.claim.as_ref().map(|(agent, session)| json!({"agent": agent, "session": session})),
            "created_by": self.created_by,
            "created_session": self.created_session,
            "created_at": iso(self.created_at),
            "updated_at": iso(self.updated_at),
            "closed_at": self.closed_at.map(iso),
            "sub_items": {
                "total": self.sub_items,
                "closed": self.sub_items_closed,
                "in_progress": self.sub_items_in_progress,
            },
            "blocked_by": self.blocked_by,
            "blocked": self.blocked(),
            "ready": self.ready,
            "project": self.project.as_ref().map(super::ProjectRef::to_json),
            "waiting": self.waiting,
            "queue": self.queue,
        })
    }
}

/// An item a review belongs to, as the review carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemTag {
    pub id: String,
    pub key: String,
    pub title: String,
}

impl ItemTag {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "key": self.key, "title": self.title})
    }

    /// The tags as `ITEMS_JSON` gives them.
    pub(super) fn list_from_column(json: Option<String>) -> Vec<Self> {
        let Some(Value::Array(list)) = json.and_then(|j| serde_json::from_str(&j).ok()) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|value| {
                let text = |k: &str| value.get(k).and_then(Value::as_str).map(str::to_string);
                Some(ItemTag {
                    id: text("id")?,
                    key: text("key")?,
                    title: text("title")?,
                })
            })
            .collect()
    }
}

/// The SQL for the items of the review `r`, as a JSON array by key.
pub(super) const ITEMS_JSON: &str = "(SELECT json_group_array(json_object('id', w.id,
     'key', wg.prefix || '-' || w.number, 'title', w.title) ORDER BY wg.prefix, w.number)
     FROM review_items x JOIN work_items w ON w.id = x.item_id JOIN repos wg ON wg.id = w.repo_id
     WHERE x.review_id = r.id)";

/// An item another one is related to: by key, with its title and status.
#[derive(Debug, Clone, PartialEq)]
pub struct Related {
    pub id: String,
    pub key: String,
    pub title: String,
    pub status: Status,
}

impl Related {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "key": self.key, "title": self.title, "status": self.status.as_str()})
    }
}

/// Which items a list shows, by where they stand. Several are any of them;
/// none is every item that is open or in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Open,
    InProgress,
    Done,
    Cancelled,
    OnHold,
    /// open, not blocked, with no open sub-item
    Ready,
    /// waiting for an item it depends on
    Blocked,
}

impl State {
    /// Every item, whatever its status: the states that cover them all once.
    pub const ALL: [State; 5] = [
        State::Open,
        State::InProgress,
        State::OnHold,
        State::Done,
        State::Cancelled,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "ready" => Some(State::Ready),
            "blocked" => Some(State::Blocked),
            other => Status::parse(other).map(|s| match s {
                Status::Open => State::Open,
                Status::InProgress => State::InProgress,
                Status::Done => State::Done,
                Status::Cancelled => State::Cancelled,
                Status::OnHold => State::OnHold,
            }),
        }
    }
}

/// What a list of items is narrowed to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemFilters {
    pub states: Vec<State>,
    /// a repository, by name
    pub repo: Option<String>,
    /// the repositories of a project, by its id
    pub project: Option<String>,
    /// the sub-items of this item, by id
    pub parent: Option<String>,
    /// `person`, or an agent's name
    pub created_by: Option<String>,
    /// words that must all be in the title or the body
    pub text: Option<String>,
    /// the items a session holds or worked on, by its agent and id
    pub session: Option<(String, String)>,
    /// only the queued items, in the queue's order
    pub queued: bool,
    /// the items without their bodies, which a list that shows none need
    /// not read
    pub summary: bool,
}

/// Why a change to the items was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// no item has this key, number or id
    NoItem(String),
    /// a number alone, without a repository to look it up in
    NumberWithoutRepo(i64),
    /// the change would make items wait for each other in a circle, given
    /// as the keys around it, the first repeated at the end
    Cycle(Vec<String>),
    /// a prefix that is not two to five capital letters
    PrefixInvalid(String),
    /// a prefix another repository has, or had
    PrefixTaken(String),
    /// no repository has this name
    NoRepo(String),
    /// two items of a breakdown have the same `ref`
    DuplicateRef(String),
    /// a breakdown names a `ref` none of its items has, nor an item
    UnknownRef(String),
    /// another session holds the item's claim: its key, the agent and the
    /// session
    ClaimedBy(String, String, Option<String>),
    /// the item is done or cancelled: open it again to work on it
    Closed(String, Status),
    /// an agent cannot mark the item done yet: the reviews and questions
    /// that wait on the person, and the sub-items still open, by key
    NotDone {
        key: String,
        waiting: Vec<String>,
        open: Vec<String>,
    },
    /// a claim, or a comment, that belongs to another agent or session
    NotYours(String),
    /// an agent cannot work on an item on hold, nor on one under it, nor
    /// change the hold: the key of the item on hold
    OnHold(String),
    /// a place in the queue named by an item that is not in it
    NotQueued(String),
    /// a place in the queue named by the item being placed
    OwnPlace(String),
    /// a file under a name the item already uses for a file of another
    /// comment or of its description: the item's key, the name, and
    /// whether the file is the description's
    FileTaken(String, String, bool),
    /// no comment of the item has this id
    NoComment(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NoItem(item) => write!(f, "no work item is {item}"),
            Refused::NumberWithoutRepo(n) => write!(
                f,
                "{n} is a number without a repository: give the item's key, such as PIN-{n}"
            ),
            Refused::Cycle(path) => write!(
                f,
                "this would make {} wait for itself: {}",
                path[0],
                path.join(" → ")
            ),
            Refused::PrefixInvalid(p) => write!(
                f,
                "`{p}` is not a prefix: a prefix is {} to {} capital letters",
                keys::PREFIX_MIN,
                keys::PREFIX_MAX
            ),
            Refused::PrefixTaken(p) => write!(f, "another repository has or had the prefix {p}"),
            Refused::NoRepo(r) => write!(f, "no repository is named {r}"),
            Refused::DuplicateRef(r) => write!(f, "two items of the breakdown have the ref `{r}`"),
            Refused::UnknownRef(r) => write!(
                f,
                "`{r}` is neither the ref of an item in the breakdown nor an item's key"
            ),
            Refused::NotQueued(key) => write!(f, "{key} is not in the queue"),
            Refused::OwnPlace(key) => write!(f, "{key} cannot be placed next to itself"),
            Refused::FileTaken(key, name, description) => write!(
                f,
                "{key} already has a file named {name}, {}: give this one another name",
                if *description {
                    "in its description"
                } else {
                    "in a comment"
                }
            ),
            Refused::OnHold(key) => {
                write!(f, "{key} is on hold: ask the person to take it off hold")
            }
            Refused::ClaimedBy(key, agent, session) => match session {
                Some(session) => write!(f, "{key} is claimed by {agent} (session {session})"),
                None => write!(f, "{key} is claimed by {agent}"),
            },
            Refused::Closed(key, status) => write!(
                f,
                "{key} is {}: open it again to work on it",
                status.as_str().replace('_', " ")
            ),
            Refused::NotDone { key, waiting, open } => {
                let mut why = Vec::new();
                if !waiting.is_empty() {
                    why.push(format!("{} still waits on the person", waiting.join(", ")));
                }
                match open.as_slice() {
                    [] => {}
                    [one] => why.push(format!("its sub-item {one} is still open")),
                    _ => why.push(format!("its sub-items {} are still open", open.join(", "))),
                }
                write!(f, "{key} is not done: {}", why.join("; "))
            }
            Refused::NotYours(what) => write!(f, "{what} belongs to another agent or session"),
            Refused::NoComment(id) => write!(f, "no comment of the item is {id}"),
        }
    }
}

pub(super) type Outcome<T> = rusqlite::Result<Result<T, Refused>>;

/// The SQL for whether the item `i` waits for an item it depends on.
pub(super) const OPEN: &str = "('open', 'in_progress')";

pub(super) fn select() -> String {
    columns(true)
}

/// The item's columns, with its body or an empty one in its place.
fn columns(body: bool) -> String {
    let body = if body { "i.body" } else { "''" };
    format!(
        "SELECT i.id, g.prefix, i.number, g.name, i.title, {body}, {status},
           (SELECT pg.prefix || '-' || p.number FROM work_items p JOIN repos pg ON pg.id = p.repo_id
             WHERE p.id = i.parent),
           i.link, i.claim_agent, i.claim_session, i.created_by, i.created_session,
           i.created_at, i.updated_at, i.closed_at,
           (SELECT COUNT(*) FROM work_items c WHERE c.parent = i.id),
           (SELECT COUNT(*) FROM work_items c WHERE c.parent = i.id AND c.status NOT IN {OPEN}),
           (SELECT group_concat(dg.prefix || '-' || d.number, ' ')
              FROM work_item_deps x JOIN work_items d ON d.id = x.depends_on
              JOIN repos dg ON dg.id = d.repo_id
             WHERE x.item_id = i.id AND d.status IN {OPEN}),
           {ready},
           {project},
           (SELECT COUNT(*) FROM reviews r LEFT JOIN outcomes o ON o.review_id = r.id
             JOIN review_items x ON x.review_id = r.id
             WHERE x.item_id = i.id AND o.review_id IS NULL
               AND (r.expires_at IS NULL OR r.expires_at > strftime('%Y-%m-%dT%H:%M:%f', 'now') || '000Z')
               AND NOT EXISTS (SELECT 1 FROM reviews n WHERE n.revises = r.id)),
           (SELECT (SELECT COUNT(*) FROM work_item_queue b WHERE b.position <= q.position)
              FROM work_item_queue q WHERE q.item_id = i.id),
           (SELECT COUNT(*) FROM work_items c WHERE c.parent = i.id AND c.status = 'in_progress')
         FROM work_items i JOIN repos g ON g.id = i.repo_id",
        status = status_of("i"),
        ready = READY,
        project = super::projects::project_json("i.repo_id"),
    )
}

/// The SQL for the status of the item `alias`, as the API gives it: an item
/// on hold is stored as open, with the time it was put on hold.
pub(super) fn status_of(alias: &str) -> String {
    format!("CASE WHEN {alias}.held_at IS NOT NULL THEN 'on_hold' ELSE {alias}.status END")
}

/// The SQL for whether the item `i` is ready: open, waiting for no item it
/// depends on, and with no open sub-item.
pub(super) const READY: &str = "(i.status = 'open' AND i.held_at IS NULL
   AND NOT EXISTS (WITH RECURSIVE up(id) AS (
          SELECT i.parent UNION SELECT a.parent FROM work_items a JOIN up ON a.id = up.id
        ) SELECT 1 FROM up JOIN work_items a ON a.id = up.id WHERE a.held_at IS NOT NULL)
   AND NOT EXISTS (SELECT 1 FROM work_item_deps x JOIN work_items d ON d.id = x.depends_on
                    WHERE x.item_id = i.id AND d.status IN ('open', 'in_progress'))
   AND NOT EXISTS (SELECT 1 FROM work_items c WHERE c.parent = i.id
                    AND c.status IN ('open', 'in_progress')))";

const BLOCKED: &str =
    "EXISTS (SELECT 1 FROM work_item_deps x JOIN work_items d ON d.id = x.depends_on
   WHERE x.item_id = i.id AND d.status IN ('open', 'in_progress'))";

pub(super) fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkItem> {
    let time = |i: usize| -> rusqlite::Result<DateTime<Utc>> {
        Ok(parse_datetime(&row.get::<_, String>(i)?).unwrap_or_default())
    };
    let mut blocked_by: Vec<String> = row
        .get::<_, Option<String>>(18)?
        .map(|keys| keys.split(' ').map(str::to_string).collect())
        .unwrap_or_default();
    blocked_by.sort_by_key(|k| sort_key(k));
    Ok(WorkItem {
        id: row.get(0)?,
        key: Key {
            prefix: row.get(1)?,
            number: row.get(2)?,
        },
        repo: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        status: Status::parse(&row.get::<_, String>(6)?).unwrap_or(Status::Open),
        parent: row.get(7)?,
        link: row.get(8)?,
        claim: row
            .get::<_, Option<String>>(9)?
            .map(|agent| Ok::<_, rusqlite::Error>((agent, row.get(10)?)))
            .transpose()?,
        created_by: row.get(11)?,
        created_session: row.get(12)?,
        created_at: time(13)?,
        updated_at: time(14)?,
        closed_at: row
            .get::<_, Option<String>>(15)?
            .and_then(|t| parse_datetime(&t)),
        sub_items: row.get::<_, i64>(16)? as usize,
        sub_items_closed: row.get::<_, i64>(17)? as usize,
        blocked_by,
        ready: row.get(19)?,
        project: super::ProjectRef::from_column(row.get(20)?),
        waiting: row.get::<_, i64>(21)? as usize,
        queue: row.get::<_, Option<i64>>(22)?.map(|n| n as usize),
        sub_items_in_progress: row.get::<_, i64>(23)? as usize,
    })
}

/// Keys in order: by prefix, then by number, not as text.
pub(super) fn sort_key(key: &str) -> (String, i64) {
    match key.parse::<Key>() {
        Ok(k) => (k.prefix, k.number),
        Err(_) => (key.to_string(), 0),
    }
}

/// The id of the item `item` names, in the repository `repo` for a number.
pub(super) fn resolve(conn: &Connection, item: &ItemRef, repo: Option<&str>) -> Outcome<String> {
    let found: Option<String> = match item {
        ItemRef::Id(id) => conn
            .query_row(
                "SELECT id FROM work_items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?,
        ItemRef::Number(n) => {
            let Some(repo) = repo else {
                return Ok(Err(Refused::NumberWithoutRepo(*n)));
            };
            by_number(
                conn,
                "(SELECT id FROM repos WHERE name = ?1)",
                &repo.to_string(),
                *n,
            )?
        }
        ItemRef::Key(key) => by_number(
            conn,
            "COALESCE((SELECT id FROM repos WHERE prefix = ?1),
                      (SELECT repo_id FROM repo_old_prefixes WHERE prefix = ?1))",
            &key.prefix,
            key.number,
        )?,
    };
    Ok(found.ok_or_else(|| Refused::NoItem(describe(item))))
}

/// The item numbered `number` in the repository the SQL `repo` finds from
/// `?1`, or the item that had that number there before it moved.
fn by_number(
    conn: &Connection,
    repo: &str,
    arg: &String,
    number: i64,
) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        &format!(
            "SELECT COALESCE(
               (SELECT id FROM work_items WHERE repo_id = {repo} AND number = ?2),
               (SELECT item_id FROM work_item_aliases WHERE repo_id = {repo} AND number = ?2))"
        ),
        params![arg, number],
        |r| r.get(0),
    )
}

fn describe(item: &ItemRef) -> String {
    match item {
        ItemRef::Key(k) => k.to_string(),
        ItemRef::Number(n) => n.to_string(),
        ItemRef::Id(id) => id.clone(),
    }
}

/// The key of an item, by its id.
pub(super) fn key_of(conn: &Connection, id: &str) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT g.prefix || '-' || i.number FROM work_items i JOIN repos g ON g.id = i.repo_id
          WHERE i.id = ?1",
        params![id],
        |r| r.get(0),
    )
}

/// What `from` waits for, directly: the items it depends on, and its
/// sub-items.
fn waits_for(conn: &Connection, from: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT depends_on FROM work_item_deps WHERE item_id = ?1
         UNION SELECT id FROM work_items WHERE parent = ?1",
    )?;
    stmt.query_map(params![from], |r| r.get(0))?.collect()
}

/// The shortest chain by which `from` waits for `to`, both ends included,
/// or `None` when it does not.
fn chain(conn: &Connection, from: &str, to: &str) -> rusqlite::Result<Option<Vec<String>>> {
    let mut came_from: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::from([from.to_string()]);
    let mut queue = VecDeque::from([from.to_string()]);
    while let Some(at) = queue.pop_front() {
        if at == to {
            let mut path = vec![at.clone()];
            let mut step = at;
            while let Some(prev) = came_from.get(&step) {
                path.push(prev.clone());
                step = prev.clone();
            }
            path.reverse();
            return Ok(Some(path));
        }
        for next in waits_for(conn, &at)? {
            if seen.insert(next.clone()) {
                came_from.insert(next.clone(), at.clone());
                queue.push_back(next);
            }
        }
    }
    Ok(None)
}

/// Whether `waiter` may wait for `on`: refused when `on` already waits for
/// `waiter`, with the circle it would close, as keys.
pub(super) fn may_wait(conn: &Connection, waiter: &str, on: &str) -> Outcome<()> {
    if waiter == on {
        let key = key_of(conn, waiter)?;
        return Ok(Err(Refused::Cycle(vec![key.clone(), key])));
    }
    match chain(conn, on, waiter)? {
        None => Ok(Ok(())),
        Some(path) => {
            let mut keys = vec![key_of(conn, waiter)?];
            for id in path {
                keys.push(key_of(conn, &id)?);
            }
            Ok(Err(Refused::Cycle(keys)))
        }
    }
}

/// The repository named `repo`, with the next number it gives an item,
/// adding the repository and giving it its prefix with its first item.
pub(super) fn next_number(
    conn: &Connection,
    repo: &str,
    at: DateTime<Utc>,
) -> rusqlite::Result<(i64, i64)> {
    let repo_id =
        super::repos::ensure(conn, Some(repo), at)?.expect("an item's repository has a name");
    let prefix: Option<String> = conn.query_row(
        "SELECT prefix FROM repos WHERE id = ?1",
        params![repo_id],
        |r| r.get(0),
    )?;
    if prefix.is_none() {
        let taken = |candidate: &str| -> bool {
            conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM repos WHERE prefix = ?1)
                     OR EXISTS (SELECT 1 FROM repo_old_prefixes WHERE prefix = ?1)",
                params![candidate],
                |r| r.get(0),
            )
            .unwrap_or(true)
        };
        let derived = keys::derive_prefix(repo, taken);
        conn.execute(
            "UPDATE repos SET prefix = ?2 WHERE id = ?1",
            params![repo_id, derived],
        )?;
    }
    let number: i64 = conn.query_row(
        "UPDATE repos SET last_item = last_item + 1 WHERE id = ?1 RETURNING last_item",
        params![repo_id],
        |r| r.get(0),
    )?;
    Ok((repo_id, number))
}

/// Adds an item in the repository `repo`, with the next number there.
fn insert(
    conn: &Connection,
    repo: &str,
    title: &str,
    body: &str,
    link: Option<&str>,
    actor: &Actor,
    at: DateTime<Utc>,
) -> rusqlite::Result<String> {
    let (repo_id, number) = next_number(conn, repo, at)?;
    let id = crate::id::next_item();
    let created = at;
    let at = iso(at);
    conn.execute(
        "INSERT INTO work_items (id, repo_id, number, title, body, status, link, created_by,
           created_session, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            repo_id,
            number,
            title,
            body,
            link,
            actor.name(),
            actor.session(),
            at
        ],
    )?;
    super::item_work::item_event(conn, &id, "created", actor, Value::Null, created)?;
    Ok(id)
}

/// Puts `item` under `parent`, or under none.
pub(super) fn set_parent_in(conn: &Connection, item: &str, parent: Option<&str>) -> Outcome<()> {
    if let Some(parent) = parent {
        if let Err(refused) = may_wait(conn, parent, item)? {
            return Ok(Err(refused));
        }
        if !stored_status(conn, item)?.is_closed()
            && let Some(refused) = closed(conn, parent)?
        {
            return Ok(Err(refused));
        }
    }
    conn.execute(
        "UPDATE work_items SET parent = ?2 WHERE id = ?1",
        params![item, parent],
    )?;
    Ok(Ok(()))
}

fn stored_status(conn: &Connection, item: &str) -> rusqlite::Result<Status> {
    let status: String = conn.query_row(
        "SELECT status FROM work_items WHERE id = ?1",
        params![item],
        |r| r.get(0),
    )?;
    Ok(Status::parse(&status).expect("a stored status is known"))
}

/// The refusal of open work under `item` when it is done or cancelled.
pub(super) fn closed(conn: &Connection, item: &str) -> rusqlite::Result<Option<Refused>> {
    let status = stored_status(conn, item)?;
    Ok(status
        .is_closed()
        .then(|| key_of(conn, item))
        .transpose()?
        .map(|key| Refused::Closed(key, status)))
}

/// Makes `item` depend on `on`.
pub(super) fn depend_in(conn: &Connection, item: &str, on: &str) -> Outcome<()> {
    if let Err(refused) = may_wait(conn, item, on)? {
        return Ok(Err(refused));
    }
    conn.execute(
        "INSERT INTO work_item_deps (item_id, depends_on) VALUES (?1, ?2)
         ON CONFLICT DO NOTHING",
        params![item, on],
    )?;
    Ok(Ok(()))
}

pub(super) fn touch(conn: &Connection, item: &str, at: DateTime<Utc>) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE work_items SET updated_at = ?2 WHERE id = ?1",
        params![item, iso(at)],
    )?;
    Ok(())
}

/// Sets an item's status, recording when it closed, or clearing that when
/// it opens again; leaving progress ends its claim.
pub(super) fn status_in(
    conn: &Connection,
    id: &str,
    status: Status,
    at: DateTime<Utc>,
) -> rusqlite::Result<()> {
    // on hold is stored as open, with the time it was put on hold
    let (stored, held) = match status {
        Status::OnHold => ("open", Some(iso(at))),
        other => (other.as_str(), None),
    };
    conn.execute(
        "UPDATE work_items SET status = ?2, updated_at = ?3,
           closed_at = CASE WHEN ?2 IN ('done', 'cancelled') THEN COALESCE(closed_at, ?3) END,
           claim_agent = CASE WHEN ?2 = 'in_progress' THEN claim_agent END,
           claim_session = CASE WHEN ?2 = 'in_progress' THEN claim_session END,
           held_at = ?4
         WHERE id = ?1",
        params![id, stored, iso(at), held],
    )?;
    // what is closed or waits leaves the queue; a claim does not
    if status.is_closed() || status == Status::OnHold {
        conn.execute(
            "DELETE FROM work_item_queue WHERE item_id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

/// Moves an item to the repository `to`: the next number there, and its
/// old number kept as an alias. Nothing happens if it is there already.
pub(super) fn move_in(
    conn: &Connection,
    id: &str,
    to: &str,
    at: DateTime<Utc>,
) -> rusqlite::Result<bool> {
    let (from_repo, from_number, from_name): (i64, i64, String) = conn.query_row(
        "SELECT i.repo_id, i.number, g.name FROM work_items i JOIN repos g ON g.id = i.repo_id
          WHERE i.id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if from_name == to {
        return Ok(false);
    }
    let (to_repo, number) = next_number(conn, to, at)?;
    conn.execute(
        "INSERT INTO work_item_aliases (repo_id, number, item_id) VALUES (?1, ?2, ?3)",
        params![from_repo, from_number, id],
    )?;
    // numbers are never given again, so an alias, even in the repository
    // it moves back to, names only this item
    conn.execute(
        "UPDATE work_items SET repo_id = ?2, number = ?3 WHERE id = ?1",
        params![id, to_repo, number],
    )?;
    touch(conn, id, at)?;
    Ok(true)
}

impl Db {
    /// The item `item` names, in the repository `repo` for a number alone.
    pub fn item(&self, item: &ItemRef, repo: Option<&str>) -> Outcome<WorkItem> {
        let conn = self.read();
        let id = match resolve(&conn, item, repo)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        conn.query_row(
            &format!("{} WHERE i.id = ?1", select()),
            params![id],
            row_to_item,
        )
        .map(Ok)
    }

    /// The items the filters keep, by repository and number.
    pub fn items(&self, filters: &ItemFilters) -> rusqlite::Result<Vec<WorkItem>> {
        let mut args: Vec<String> = Vec::new();
        let mut wheres: Vec<String> = Vec::new();
        let states: Vec<State> = if filters.states.is_empty() {
            vec![State::Open, State::InProgress, State::OnHold]
        } else {
            filters.states.clone()
        };
        let any: Vec<String> = states
            .iter()
            .map(|s| match s {
                State::Open => "(i.status = 'open' AND i.held_at IS NULL)".to_string(),
                State::OnHold => "i.held_at IS NOT NULL".to_string(),
                State::InProgress => "i.status = 'in_progress'".to_string(),
                State::Done => "i.status = 'done'".to_string(),
                State::Cancelled => "i.status = 'cancelled'".to_string(),
                State::Ready => READY.to_string(),
                State::Blocked => format!("(i.status IN {OPEN} AND {BLOCKED})"),
            })
            .collect();
        wheres.push(format!("({})", any.join(" OR ")));
        if let Some(repo) = &filters.repo {
            args.push(repo.clone());
            wheres.push(format!("g.name = ?{}", args.len()));
        }
        if let Some(project) = &filters.project {
            args.push(project.clone());
            wheres.push(format!("g.project_id = ?{}", args.len()));
        }
        if let Some(parent) = &filters.parent {
            args.push(parent.clone());
            wheres.push(format!("i.parent = ?{}", args.len()));
        }
        if let Some(by) = &filters.created_by {
            args.push(by.clone());
            wheres.push(format!("i.created_by = ?{}", args.len()));
        }
        if let Some((agent, session)) = &filters.session {
            args.push(agent.clone());
            let a = args.len();
            args.push(session.clone());
            let s = args.len();
            // the items it holds, added, changed, commented on or sent
            // reviews for, each found through its own index
            wheres.push(format!(
                "i.id IN (
                   SELECT id FROM work_items WHERE claim_agent = ?{a} AND claim_session = ?{s}
                   UNION SELECT id FROM work_items WHERE created_by = ?{a} AND created_session = ?{s}
                   UNION SELECT item_id FROM work_item_events WHERE actor = ?{a} AND session = ?{s}
                   UNION SELECT item_id FROM work_item_comments WHERE author = ?{a} AND session = ?{s}
                   UNION SELECT x.item_id FROM reviews r JOIN review_items x ON x.review_id = r.id
                          WHERE r.requested_by = ?{a} AND r.session = ?{s})"
            ));
        }
        for word in filters
            .text
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
        {
            let escaped = word
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            args.push(format!("%{escaped}%"));
            let n = args.len();
            wheres.push(format!(
                "(i.title LIKE ?{n} ESCAPE '\\' OR i.body LIKE ?{n} ESCAPE '\\')"
            ));
        }
        let order = if filters.queued {
            wheres.push("EXISTS (SELECT 1 FROM work_item_queue q WHERE q.item_id = i.id)".into());
            "(SELECT q.position FROM work_item_queue q WHERE q.item_id = i.id)"
        } else {
            "g.name, i.number"
        };
        let sql = format!(
            "{} WHERE {} ORDER BY {order}",
            columns(!filters.summary),
            wheres.join(" AND ")
        );
        let conn = self.read();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), row_to_item)?;
        rows.collect()
    }

    /// What an item depends on, and what depends on it, each by key.
    pub fn relations(&self, id: &str) -> rusqlite::Result<(Vec<Related>, Vec<Related>)> {
        let conn = self.read();
        let related = |sql: &str| -> rusqlite::Result<Vec<Related>> {
            let mut stmt = conn.prepare(sql)?;
            let mut rows: Vec<Related> = stmt
                .query_map(params![id], |r| {
                    Ok(Related {
                        id: r.get(0)?,
                        key: r.get(1)?,
                        title: r.get(2)?,
                        status: Status::parse(&r.get::<_, String>(3)?).unwrap_or(Status::Open),
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            rows.sort_by_key(|r| sort_key(&r.key));
            Ok(rows)
        };
        Ok((
            related(
                "SELECT d.id, g.prefix || '-' || d.number, d.title,
                   CASE WHEN d.held_at IS NOT NULL THEN 'on_hold' ELSE d.status END
                   FROM work_item_deps x JOIN work_items d ON d.id = x.depends_on
                   JOIN repos g ON g.id = d.repo_id WHERE x.item_id = ?1",
            )?,
            related(
                "SELECT d.id, g.prefix || '-' || d.number, d.title,
                   CASE WHEN d.held_at IS NOT NULL THEN 'on_hold' ELSE d.status END
                   FROM work_item_deps x JOIN work_items d ON d.id = x.item_id
                   JOIN repos g ON g.id = d.repo_id WHERE x.depends_on = ?1",
            )?,
        ))
    }

    /// Adds an item, under its parent and depending on its dependencies, or
    /// nothing at all when one of them is refused.
    pub fn add_item(&self, new: &NewItem, actor: &Actor, at: DateTime<Utc>) -> Outcome<WorkItem> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = insert(
            &tx,
            &new.repo,
            &new.title,
            &new.body,
            new.link.as_deref(),
            actor,
            at,
        )?;
        if let Some(parent) = &new.parent {
            let parent = match resolve(&tx, parent, Some(&new.repo))? {
                Ok(p) => p,
                Err(refused) => return Ok(Err(refused)),
            };
            if let Err(refused) = set_parent_in(&tx, &id, Some(&parent))? {
                return Ok(Err(refused));
            }
        }
        for on in &new.depends_on {
            let on = match resolve(&tx, on, Some(&new.repo))? {
                Ok(o) => o,
                Err(refused) => return Ok(Err(refused)),
            };
            if let Err(refused) = depend_in(&tx, &id, &on)? {
                return Ok(Err(refused));
            }
        }
        let item = tx.query_row(
            &format!("{} WHERE i.id = ?1", select()),
            params![id],
            row_to_item,
        )?;
        tx.commit()?;
        Ok(Ok(item))
    }

    /// Adds every item of a breakdown, or none: each with its `ref`, in the
    /// order given.
    pub fn add_breakdown(
        &self,
        breakdown: &Breakdown,
        actor: &Actor,
        at: DateTime<Utc>,
    ) -> Outcome<Vec<(String, WorkItem)>> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut ids: HashMap<&str, String> = HashMap::new();
        for item in &breakdown.items {
            if ids.contains_key(item.r#ref.as_str()) {
                return Ok(Err(Refused::DuplicateRef(item.r#ref.clone())));
            }
            let id = insert(
                &tx,
                &breakdown.repo,
                &item.title,
                &item.body,
                item.link.as_deref(),
                actor,
                at,
            )?;
            ids.insert(item.r#ref.as_str(), id);
        }
        // a `ref` of the breakdown, or an existing item
        let find = |name: &str| -> Outcome<String> {
            if let Some(id) = ids.get(name) {
                return Ok(Ok(id.clone()));
            }
            match ItemRef::parse(name) {
                Ok(item) => resolve(&tx, &item, Some(&breakdown.repo)),
                Err(_) => Ok(Err(Refused::UnknownRef(name.to_string()))),
            }
        };
        let top = match &breakdown.parent {
            Some(parent) => match resolve(&tx, parent, Some(&breakdown.repo))? {
                Ok(p) => Some(p),
                Err(refused) => return Ok(Err(refused)),
            },
            None => None,
        };
        for item in &breakdown.items {
            let id = &ids[item.r#ref.as_str()];
            let parent = match &item.parent {
                Some(name) => match find(name)? {
                    Ok(p) => Some(p),
                    Err(refused) => return Ok(Err(refused)),
                },
                None => top.clone(),
            };
            if let Some(parent) = parent
                && let Err(refused) = set_parent_in(&tx, id, Some(&parent))?
            {
                return Ok(Err(refused));
            }
        }
        for item in &breakdown.items {
            let id = &ids[item.r#ref.as_str()];
            for name in &item.depends_on {
                let on = match find(name)? {
                    Ok(o) => o,
                    Err(refused) => return Ok(Err(refused)),
                };
                if let Err(refused) = depend_in(&tx, id, &on)? {
                    return Ok(Err(refused));
                }
            }
        }
        let mut added = Vec::new();
        for item in &breakdown.items {
            let row = tx.query_row(
                &format!("{} WHERE i.id = ?1", select()),
                params![ids[item.r#ref.as_str()]],
                row_to_item,
            )?;
            added.push((item.r#ref.clone(), row));
        }
        tx.commit()?;
        Ok(Ok(added))
    }

    /// Sets a repository's prefix. Its old one still finds its items, and
    /// no other repository can take it.
    pub fn set_repo_prefix(&self, repo: &str, prefix: &str) -> Outcome<()> {
        if !keys::valid_prefix(prefix) {
            return Ok(Err(Refused::PrefixInvalid(prefix.to_string())));
        }
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some((id, old)): Option<(i64, Option<String>)> = tx
            .query_row(
                "SELECT id, prefix FROM repos WHERE name = ?1",
                params![repo],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        else {
            return Ok(Err(Refused::NoRepo(repo.to_string())));
        };
        let taken: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM repos WHERE prefix = ?1 AND id <> ?2)
                 OR EXISTS (SELECT 1 FROM repo_old_prefixes WHERE prefix = ?1 AND repo_id <> ?2)",
            params![prefix, id],
            |r| r.get(0),
        )?;
        if taken {
            return Ok(Err(Refused::PrefixTaken(prefix.to_string())));
        }
        if let Some(old) = old.filter(|o| o != prefix) {
            tx.execute(
                "INSERT INTO repo_old_prefixes (prefix, repo_id) VALUES (?1, ?2)",
                params![old, id],
            )?;
        }
        tx.execute(
            "DELETE FROM repo_old_prefixes WHERE prefix = ?1",
            params![prefix],
        )?;
        tx.execute(
            "UPDATE repos SET prefix = ?2 WHERE id = ?1",
            params![id, prefix],
        )?;
        tx.commit()?;
        Ok(Ok(()))
    }

    /// Deletes an item and its sub-items, at any depth. Returns how many
    /// went, and those that were in progress, each with who held it.
    pub fn delete_item(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
    ) -> Outcome<(usize, Vec<(Related, super::item_work::HeldBy)>)> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = match resolve(&tx, item, repo)? {
            Ok(id) => id,
            Err(refused) => return Ok(Err(refused)),
        };
        let mut stmt = tx.prepare(
            "WITH RECURSIVE tree(id) AS (
               SELECT ?1 UNION SELECT w.id FROM work_items w JOIN tree ON w.parent = tree.id
             ) SELECT w.id, w.status, w.claim_agent, w.claim_session
                 FROM work_items w JOIN tree ON tree.id = w.id",
        )?;
        type Row = (String, String, Option<String>, Option<String>);
        let tree: Vec<Row> = stmt
            .query_map(params![id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let mut held = Vec::new();
        for (sub, status, agent, session) in &tree {
            if status == "in_progress"
                && let Some(agent) = agent
            {
                let related = super::item_work::related(&tx, sub)?;
                held.push((related, Some((agent.clone(), session.clone()))));
            }
        }
        held.sort_by_key(|(r, _)| sort_key(&r.key));
        tx.execute("DELETE FROM work_items WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(Ok((tree.len(), held)))
    }
}
