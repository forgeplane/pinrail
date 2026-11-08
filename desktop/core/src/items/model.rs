//! What work items are made of, apart from how they are stored.

use super::keys::{Key, parse_number};

/// Where an item stands. Blocked and ready are derived, not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    InProgress,
    Done,
    Cancelled,
    /// kept, but not to be worked on until the person takes it off hold;
    /// stored as `open` with `held_at` set
    OnHold,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Done => "done",
            Status::Cancelled => "cancelled",
            Status::OnHold => "on_hold",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "open" => Some(Status::Open),
            "in_progress" => Some(Status::InProgress),
            "done" => Some(Status::Done),
            "cancelled" => Some(Status::Cancelled),
            "on_hold" => Some(Status::OnHold),
            _ => None,
        }
    }

    /// Done or cancelled: nothing more will happen to it.
    pub fn is_closed(self) -> bool {
        matches!(self, Status::Done | Status::Cancelled)
    }
}

/// Who made a change: the person, or an agent in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Person,
    Agent {
        name: String,
        session: Option<String>,
    },
}

impl Actor {
    /// The name stored for it: `person`, or the agent's.
    pub fn name(&self) -> &str {
        match self {
            Actor::Person => "person",
            Actor::Agent { name, .. } => name,
        }
    }

    pub fn session(&self) -> Option<&str> {
        match self {
            Actor::Person => None,
            Actor::Agent { session, .. } => session.as_deref(),
        }
    }
}

/// How an item is named in a request: its key, its number in a repository
/// the request gives, or its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRef {
    Key(Key),
    Number(i64),
    Id(String),
}

impl ItemRef {
    /// `PIN-42`, `42`, `#42` or `w_…`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.starts_with("w_") {
            return Ok(ItemRef::Id(text.to_string()));
        }
        if let Some(number) = parse_number(text) {
            return Ok(ItemRef::Number(number));
        }
        text.parse::<Key>().map(ItemRef::Key)
    }
}

/// An item to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewItem {
    pub repo: String,
    pub title: String,
    pub body: String,
    pub parent: Option<ItemRef>,
    pub depends_on: Vec<ItemRef>,
    pub link: Option<String>,
}

/// Items added together: each names the others by a `ref` of the request's
/// own, before they have keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakdown {
    pub repo: String,
    /// an existing item that every item without a parent goes under
    pub parent: Option<ItemRef>,
    pub items: Vec<BreakdownItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakdownItem {
    pub r#ref: String,
    pub title: String,
    pub body: String,
    /// another item's `ref`, or an existing item
    pub parent: Option<String>,
    /// other items' `ref`s, or existing items
    pub depends_on: Vec<String>,
    pub link: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_is_named_by_its_key_its_number_or_its_id() {
        assert_eq!(
            ItemRef::parse("PIN-42").unwrap(),
            ItemRef::Key(Key {
                prefix: "PIN".into(),
                number: 42
            })
        );
        assert_eq!(ItemRef::parse("42").unwrap(), ItemRef::Number(42));
        assert_eq!(ItemRef::parse(" #42 ").unwrap(), ItemRef::Number(42));
        assert_eq!(
            ItemRef::parse("w_01ABC").unwrap(),
            ItemRef::Id("w_01ABC".into())
        );
        for bad in ["pin-42", "0", "PIN", "", "r_01ABC"] {
            assert!(ItemRef::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_status_is_stored_as_its_name() {
        for status in [
            Status::Open,
            Status::InProgress,
            Status::Done,
            Status::Cancelled,
        ] {
            assert_eq!(Status::parse(status.as_str()), Some(status));
        }
        assert_eq!(Status::parse("blocked"), None);
        assert!(Status::Done.is_closed() && Status::Cancelled.is_closed());
        assert!(!Status::Open.is_closed() && !Status::InProgress.is_closed());
    }
}
