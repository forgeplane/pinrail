//! Work items as an application operation: what the API and the CLI ask
//! for, checked, applied through the database, and announced.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};

use super::{Actor, Breakdown, ItemRef, NewItem, Status};
use crate::attachments::{Attachments, ReviewAttachment};
use crate::db::{Claim, Db, Edited, ItemChange, ItemFilters, Refused, WorkItem};
use crate::error::Error;
use crate::events::{self, Bus, Notice};

/// The longest title an item takes, as a review's.
pub const TITLE_MAX: usize = 200;
/// The longest body an item takes: the goal and its context, in Markdown.
pub const BODY_MAX: usize = 100_000;
/// The longest comment, as a note.
pub const COMMENT_MAX: usize = 10_000;
/// The longest link.
pub const LINK_MAX: usize = 2_000;
/// The most items a breakdown adds at once.
pub const BREAKDOWN_MAX: usize = 100;

#[derive(Debug)]
pub struct Items {
    db: Arc<Db>,
    bus: Bus,
    /// the person's name, which their comments are delivered under
    user: String,
    attachments: Attachments,
}

/// A refusal as the API answers it: an unknown item is not found, a rule
/// it breaks is a conflict, and a malformed request is invalid.
fn refused(refused: Refused) -> Error {
    match refused {
        Refused::NoItem(_) | Refused::NoRepo(_) | Refused::NoComment(_) => {
            Error::NotFound(refused.to_string().replacen("no ", "", 1))
        }
        Refused::NumberWithoutRepo(_)
        | Refused::PrefixInvalid(_)
        | Refused::DuplicateRef(_)
        | Refused::UnknownRef(_) => Error::invalid("", refused.to_string()),
        Refused::NotYours(_) => Error::Forbidden(refused.to_string()),
        Refused::Cycle(_)
        | Refused::PrefixTaken(_)
        | Refused::ClaimedBy(..)
        | Refused::Closed(..)
        | Refused::NotDone { .. } => Error::Conflict(refused.to_string()),
    }
}

fn check_title(path: &str, title: &str) -> Result<String, Error> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::invalid(path, "is required"));
    }
    if title.contains(['\n', '\r']) {
        return Err(Error::invalid(path, "must be one line"));
    }
    if title.chars().count() > TITLE_MAX {
        return Err(Error::invalid(
            path,
            format!("must be at most {TITLE_MAX} characters"),
        ));
    }
    Ok(title.to_string())
}

fn check_body(path: &str, body: &str) -> Result<String, Error> {
    if body.chars().count() > BODY_MAX {
        return Err(Error::invalid(
            path,
            format!("must be at most {BODY_MAX} characters"),
        ));
    }
    Ok(body.to_string())
}

fn check_link(path: &str, link: &str) -> Result<String, Error> {
    let link = link.trim();
    if !(link.starts_with("https://") || link.starts_with("http://")) {
        return Err(Error::invalid(path, "must be an http or https URL"));
    }
    if link.len() > LINK_MAX || link.contains(char::is_whitespace) {
        return Err(Error::invalid(path, "is not a URL"));
    }
    Ok(link.to_string())
}

fn check_repo(repo: &str) -> Result<String, Error> {
    let repo = repo.trim();
    if repo.is_empty() {
        return Err(Error::invalid(
            "/repo",
            "name the repository, such as forgeplane/pinrail",
        ));
    }
    if repo.chars().count() > crate::projects::MAX_REPO {
        return Err(Error::invalid(
            "/repo",
            format!("must be at most {} characters", crate::projects::MAX_REPO),
        ));
    }
    Ok(repo.to_string())
}

fn check_comment(path: &str, text: &str) -> Result<String, Error> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::invalid(path, "write the comment"));
    }
    if text.chars().count() > COMMENT_MAX {
        return Err(Error::invalid(
            path,
            format!("must be at most {COMMENT_MAX} characters"),
        ));
    }
    Ok(text.to_string())
}

impl Items {
    pub(crate) fn new(db: Arc<Db>, bus: Bus, user: String, attachments: Attachments) -> Self {
        Items {
            db,
            bus,
            user,
            attachments,
        }
    }

    fn announce(&self, kind: &str, item: &WorkItem) -> Result<(), Error> {
        let event_id = self.db.append_event(
            None,
            kind,
            None,
            &json!({"id": item.id, "key": item.key.to_string()}),
        )?;
        self.bus.publish(Notice {
            event_id,
            kind: kind.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: None,
        });
        Ok(())
    }

    /// The items the filters keep.
    pub fn list(&self, filters: &ItemFilters) -> Result<Vec<WorkItem>, Error> {
        Ok(self.db.items(filters)?)
    }

    /// An item, named by its key, its id, or its number in `repo`.
    pub fn get(&self, item: &ItemRef, repo: Option<&str>) -> Result<WorkItem, Error> {
        self.db.item(item, repo)?.map_err(refused)
    }

    /// What an agent's session did to work items, with their items,
    /// oldest first.
    pub fn session_item_events(
        &self,
        agent: &str,
        session: &str,
    ) -> Result<Vec<(crate::db::Entry, crate::db::ItemTag)>, Error> {
        Ok(self.db.session_item_events(agent, session)?)
    }

    /// The comments an agent's session wrote on work items, with their
    /// items, oldest first.
    pub fn session_comments(
        &self,
        agent: &str,
        session: &str,
    ) -> Result<Vec<(crate::db::Comment, crate::db::ItemTag)>, Error> {
        Ok(self.db.session_comments(agent, session)?)
    }

    /// An item with everything about it: its relations, sub-items, files,
    /// reviews, sessions and timeline.
    pub fn detail(&self, item: &WorkItem) -> Result<Value, Error> {
        let (depends_on, blocks) = self.db.relations(&item.id)?;
        let sub_items = self.db.items(&ItemFilters {
            parent: Some(item.id.clone()),
            states: vec![
                crate::db::State::Open,
                crate::db::State::InProgress,
                crate::db::State::Done,
                crate::db::State::Cancelled,
            ],
            ..Default::default()
        })?;
        let parents = self.ancestors(item)?;
        let mut value = item.to_json();
        value["ancestors"] = json!(parents);
        value["depends_on"] = json!(depends_on.iter().map(|r| r.to_json()).collect::<Vec<_>>());
        value["blocks"] = json!(blocks.iter().map(|r| r.to_json()).collect::<Vec<_>>());
        value["sub_items_list"] = json!(
            sub_items
                .iter()
                .map(|s| json!({"id": s.id, "key": s.key.to_string(), "title": s.title, "status": s.status.as_str()}))
                .collect::<Vec<_>>()
        );
        value["files"] = json!(
            self.db
                .item_body_files(&item.id)?
                .iter()
                .map(|f| json!({"name": f.name, "sha256": f.sha256, "size": f.size, "media_type": f.media_type}))
                .collect::<Vec<_>>()
        );
        value["reviews"] = json!(
            self.db
                .item_reviews(&item.id, Utc::now())?
                .iter()
                .map(|r| r.to_json())
                .collect::<Vec<_>>()
        );
        value["sessions"] = json!(
            self.db
                .item_sessions(&item.id)?
                .iter()
                .map(|s| s.to_json())
                .collect::<Vec<_>>()
        );
        value["timeline"] = json!(
            self.db
                .item_timeline(&item.id)?
                .iter()
                .map(|e| e.to_json())
                .collect::<Vec<_>>()
        );
        Ok(value)
    }

    /// The item's parents, nearest last, by key and title.
    fn ancestors(&self, item: &WorkItem) -> Result<Vec<Value>, Error> {
        let mut chain = Vec::new();
        let mut parent = item.parent.clone();
        while let Some(key) = parent {
            let Ok(found) = self
                .db
                .item(&ItemRef::parse(&key).map_err(Error::Internal)?, None)?
            else {
                break;
            };
            chain.push(json!({"key": key, "title": found.title}));
            parent = found.parent;
            if chain.len() > 100 {
                break;
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// Adds an item, with files for its body already in the blob store.
    pub fn add(
        &self,
        new: &NewItem,
        attachments: Option<&Value>,
        actor: &Actor,
    ) -> Result<WorkItem, Error> {
        let mut new = new.clone();
        new.repo = check_repo(&new.repo)?;
        new.title = check_title("/title", &new.title)?;
        new.body = check_body("/body", &new.body)?;
        new.link = new
            .link
            .as_deref()
            .map(|l| check_link("/link", l))
            .transpose()?;
        let files = self.files(attachments, &new.body, "the description")?;
        let item = self
            .db
            .add_item(&new, actor, Utc::now())?
            .map_err(refused)?;
        let item = if files.is_empty() {
            item
        } else {
            self.edit(
                &ItemRef::Id(item.id.clone()),
                None,
                &ItemChange {
                    files,
                    ..Default::default()
                },
                actor,
            )?
            .item
        };
        self.announce(events::ITEM_CHANGED, &item)?;
        Ok(item)
    }

    /// Adds every item of a breakdown, or none.
    pub fn add_breakdown(
        &self,
        breakdown: &Breakdown,
        actor: &Actor,
    ) -> Result<Vec<(String, WorkItem)>, Error> {
        let mut breakdown = breakdown.clone();
        breakdown.repo = check_repo(&breakdown.repo)?;
        if breakdown.items.is_empty() {
            return Err(Error::invalid("/items", "list at least one item"));
        }
        if breakdown.items.len() > BREAKDOWN_MAX {
            return Err(Error::invalid(
                "/items",
                format!("can hold at most {BREAKDOWN_MAX} items"),
            ));
        }
        for (i, item) in breakdown.items.iter_mut().enumerate() {
            if item.r#ref.trim().is_empty() {
                return Err(Error::invalid(format!("/items/{i}/ref"), "is required"));
            }
            item.title = check_title(&format!("/items/{i}/title"), &item.title)?;
            item.body = check_body(&format!("/items/{i}/body"), &item.body)?;
            item.link = item
                .link
                .as_deref()
                .map(|l| check_link(&format!("/items/{i}/link"), l))
                .transpose()?;
        }
        let added = self
            .db
            .add_breakdown(&breakdown, actor, Utc::now())?
            .map_err(refused)?;
        if let Some((_, first)) = added.first() {
            self.announce(events::ITEM_CHANGED, first)?;
        }
        Ok(added)
    }

    /// The next ready item of a repository, claimed for `actor` with
    /// `claim`; `None` when nothing is ready.
    pub fn next(&self, repo: &str, claim: bool, actor: &Actor) -> Result<Option<WorkItem>, Error> {
        let repo = check_repo(repo)?;
        let item = self.db.next_item(&repo, actor, claim, Utc::now())?;
        if claim && let Some(item) = &item {
            self.announce(events::ITEM_CHANGED, item)?;
        }
        Ok(item)
    }

    /// Changes an item, whole or not at all. An agent cancels only with a
    /// comment that says why.
    pub fn edit(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        change: &ItemChange,
        actor: &Actor,
    ) -> Result<Edited, Error> {
        let mut change = change.clone();
        if let Some(title) = &change.title {
            change.title = Some(check_title("/title", title)?);
        }
        if let Some(body) = &change.body {
            change.body = Some(check_body("/body", body)?);
        }
        if let Some(Some(link)) = &change.link {
            change.link = Some(Some(check_link("/link", link)?));
        }
        if let Some(to) = &change.repo {
            change.repo = Some(check_repo(to)?);
        }
        if let Some(text) = &change.comment {
            change.comment = Some(check_comment("/comment", text)?);
        }
        if change.status == Some(Status::Cancelled)
            && matches!(actor, Actor::Agent { .. })
            && change.comment.is_none()
        {
            return Err(Error::invalid(
                "/comment",
                "say why the item is cancelled, such as a duplicate of another",
            ));
        }
        let edited = self
            .db
            .edit_item(item, repo, &change, actor, Utc::now())?
            .map_err(refused)?;
        self.announce(events::ITEM_CHANGED, &edited.item)?;
        Ok(edited)
    }

    /// Marks an item done, with a comment that says what was done.
    pub fn done(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        comment: Option<&str>,
        actor: &Actor,
    ) -> Result<Edited, Error> {
        self.edit(
            item,
            repo,
            &ItemChange {
                status: Some(Status::Done),
                comment: comment.map(str::to_string),
                ..Default::default()
            },
            actor,
        )
    }

    /// Claims an item, or gives up its claim.
    pub fn claim(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        claim: Claim,
        actor: &Actor,
    ) -> Result<Edited, Error> {
        self.edit(
            item,
            repo,
            &ItemChange {
                claim: Some(claim),
                ..Default::default()
            },
            actor,
        )
    }

    /// Adds a comment, with files already in the blob store. The person's
    /// comment on an item an agent's session holds also reaches that
    /// session with its next status.
    pub fn comment(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        body: &str,
        attachments: Option<&Value>,
        actor: &Actor,
    ) -> Result<crate::db::Comment, Error> {
        let body = check_comment("/body", body)?;
        let files = self.files(attachments, &body, "the comment")?;
        let (item, comment) = self
            .db
            .add_item_comment(item, repo, &body, &files, actor, Utc::now())?
            .map_err(refused)?;
        if *actor == Actor::Person
            && let Some((agent, Some(session))) = &item.claim
            && agent != "person"
        {
            let text = format!(
                "Comment on {} {}:\n\n{}",
                item.key, item.title, comment.body
            );
            if let Some((message, event_id)) =
                self.db.add_message(agent, session, &text, &self.user)?
            {
                self.bus.publish(Notice {
                    event_id,
                    kind: events::SESSION_MESSAGE_SENT.to_string(),
                    review_id: None,
                    review: None,
                    keys: None,
                    session: Some(crate::db::message_notice(agent, session, &message)),
                });
            }
        }
        self.announce(events::ITEM_CHANGED, &item)?;
        Ok(comment)
    }

    /// Changes a comment's text; its author alone can.
    pub fn edit_comment(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        comment: &str,
        body: &str,
        actor: &Actor,
    ) -> Result<crate::db::Comment, Error> {
        let body = check_comment("/body", body)?;
        let edited = self
            .db
            .edit_item_comment(item, repo, comment, &body, actor, Utc::now())?
            .map_err(refused)?;
        self.announce(events::ITEM_CHANGED, &self.get(item, repo)?)?;
        Ok(edited)
    }

    /// Deletes a comment; its author can, and the person can delete any.
    pub fn delete_comment(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        comment: &str,
        actor: &Actor,
    ) -> Result<(), Error> {
        self.db
            .delete_item_comment(item, repo, comment, actor)?
            .map_err(refused)?;
        self.announce(events::ITEM_CHANGED, &self.get(item, repo)?)?;
        Ok(())
    }

    /// Deletes an item and its sub-items: the person alone can. Returns how
    /// many went.
    pub fn delete(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        actor: &Actor,
    ) -> Result<usize, Error> {
        if *actor != Actor::Person {
            return Err(Error::Forbidden(
                "only the person deletes a work item: cancel it with a comment instead".into(),
            ));
        }
        let found = self.get(item, repo)?;
        let count = self.db.delete_item(item, repo)?.map_err(refused)?;
        self.announce(events::ITEM_DELETED, &found)?;
        Ok(count)
    }

    /// Puts every round of a review under exactly `items`, and announces
    /// each item it joined or left.
    pub fn set_review_items(
        &self,
        review: &str,
        items: &[ItemRef],
        actor: &Actor,
    ) -> Result<(), Error> {
        let rounds: Vec<String> = self.db.rounds(review)?.into_iter().map(|r| r.id).collect();
        if rounds.is_empty() {
            return Err(Error::NotFound(format!("review {review}")));
        }
        let changed = self
            .db
            .set_review_items(&rounds, items, actor, Utc::now())?
            .map_err(refused)?;
        for id in changed {
            self.announce(events::ITEM_CHANGED, &self.get(&ItemRef::Id(id), None)?)?;
        }
        Ok(())
    }

    /// Sets a repository's key prefix.
    pub fn set_prefix(&self, repo: &str, prefix: &str) -> Result<(), Error> {
        self.db
            .set_repo_prefix(repo, prefix.trim())?
            .map_err(refused)?;
        let event_id = self.db.append_event(
            None,
            events::ITEM_CHANGED,
            None,
            &json!({"repo": repo, "prefix": prefix}),
        )?;
        self.bus.publish(Notice {
            event_id,
            kind: events::ITEM_CHANGED.to_string(),
            review_id: None,
            review: None,
            keys: None,
            session: None,
        });
        Ok(())
    }

    /// One of an item's files, by name.
    pub fn file(
        &self,
        item: &ItemRef,
        repo: Option<&str>,
        name: &str,
    ) -> Result<ReviewAttachment, Error> {
        let found = self.get(item, repo)?;
        self.db
            .item_file(&found.id, name)?
            .ok_or_else(|| Error::NotFound(format!("file {name} of {}", found.key)))
    }

    /// Files sent beside a change to an item's body, already in the blob
    /// store. The body may show files it was given before, so only the
    /// files themselves are checked.
    pub fn check_files(&self, attachments: Option<&Value>) -> Result<Vec<ReviewAttachment>, Error> {
        self.attachments
            .check_note(attachments, "", ("/body", "the description"))
    }

    /// Files sent beside a body or a comment, checked as a note's are.
    /// `what` names the Markdown, such as "the comment".
    fn files(
        &self,
        attachments: Option<&Value>,
        markdown: &str,
        what: &str,
    ) -> Result<Vec<ReviewAttachment>, Error> {
        // a body that shows files it does not carry is checked all the same
        let declared = attachments.filter(|a| !a.is_null());
        self.attachments
            .check_note(declared, markdown, ("/body", what))
    }
}
