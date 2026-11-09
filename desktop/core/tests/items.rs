//! Work items as stored: keys and numbering, the tree, dependencies, what
//! is blocked and ready, listing, moving, prefixes and breakdowns.

use chrono::Utc;
use pinrail_core::db::{Db, ItemChange, ItemFilters, Refused, State, WorkItem};
use pinrail_core::items::{Actor, Breakdown, BreakdownItem, ItemRef, NewItem, Status};

struct Store {
    _dir: tempfile::TempDir,
    db: Db,
    path: std::path::PathBuf,
}

fn store() -> Store {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pinrail.db");
    let db = Db::open(&path).unwrap();
    Store {
        _dir: dir,
        db,
        path,
    }
}

fn agent() -> Actor {
    Actor::Agent {
        name: "claude-code".into(),
        session: Some("s-1".into()),
    }
}

fn r(text: &str) -> ItemRef {
    ItemRef::parse(text).unwrap()
}

fn new(repo: &str, title: &str) -> NewItem {
    NewItem {
        repo: repo.into(),
        title: title.into(),
        body: String::new(),
        parent: None,
        depends_on: Vec::new(),
        link: None,
    }
}

/// Adds an item and returns its key.
fn add(s: &Store, repo: &str, title: &str) -> String {
    s.db.add_item(&new(repo, title), &agent(), Utc::now())
        .unwrap()
        .unwrap()
        .key
        .to_string()
}

fn get(s: &Store, key: &str) -> WorkItem {
    s.db.item(&r(key), None).unwrap().unwrap()
}

/// A change the person makes to an item, through the same path as the app.
fn edit(s: &Store, item: &str, change: ItemChange) -> Result<WorkItem, Refused> {
    s.db.edit_item(&r(item), None, &change, &Actor::Person, Utc::now())
        .unwrap()
        .map(|edited| edited.item)
}

fn depend(s: &Store, item: &str, on: &str) -> Result<(), Refused> {
    edit(
        s,
        item,
        ItemChange {
            depends_on: vec![r(on)],
            ..Default::default()
        },
    )
    .map(|_| ())
}

fn under(s: &Store, item: &str, parent: &str) -> Result<(), Refused> {
    edit(
        s,
        item,
        ItemChange {
            parent: Some(Some(r(parent))),
            ..Default::default()
        },
    )
    .map(|_| ())
}

fn status(s: &Store, key: &str, status: Status) {
    edit(
        s,
        key,
        ItemChange {
            status: Some(status),
            ..Default::default()
        },
    )
    .unwrap();
}

fn move_to(s: &Store, item: &str, repo: &str) -> WorkItem {
    edit(
        s,
        item,
        ItemChange {
            repo: Some(repo.into()),
            ..Default::default()
        },
    )
    .unwrap()
}

fn keys(items: &[WorkItem]) -> Vec<String> {
    items.iter().map(|i| i.key.to_string()).collect()
}

fn list(s: &Store, states: &[State]) -> Vec<String> {
    keys(
        &s.db
            .items(&ItemFilters {
                states: states.to_vec(),
                ..Default::default()
            })
            .unwrap(),
    )
}

fn cycle(path: &[&str]) -> Refused {
    Refused::Cycle(path.iter().map(|k| k.to_string()).collect())
}

// --- keys and numbering

#[test]
fn a_repository_numbers_its_items_from_one_under_its_prefix() {
    let s = store();
    assert_eq!(add(&s, "forgeplane/pinrail", "First"), "PIN-1");
    assert_eq!(add(&s, "forgeplane/pinrail", "Second"), "PIN-2");
    assert_eq!(add(&s, "forgeplane/pinrail-plugins", "Other"), "PP-1");
    assert_eq!(add(&s, "forgeplane/pinrail", "Third"), "PIN-3");
    let item = get(&s, "PIN-2");
    assert_eq!(item.title, "Second");
    assert_eq!(item.repo, "forgeplane/pinrail");
    assert_eq!(item.status, Status::Open);
    assert_eq!(item.created_by, "claude-code");
    assert_eq!(item.created_session.as_deref(), Some("s-1"));
    assert!(item.id.starts_with("w_"));
}

#[test]
fn a_deleted_items_number_is_not_given_again() {
    let s = store();
    add(&s, "pinrail", "One");
    add(&s, "pinrail", "Two");
    s.db.delete_item(&r("PIN-2"), None).unwrap().unwrap();
    assert_eq!(add(&s, "pinrail", "Three"), "PIN-3");
}

#[test]
fn a_prefix_another_repository_has_gives_the_next_one() {
    let s = store();
    assert_eq!(add(&s, "acme/pinrail", "A"), "PIN-1");
    assert_eq!(add(&s, "forgeplane/pinrail", "B"), "PINR-1");
    assert_eq!(add(&s, "pinrail", "C"), "PINRA-1");
}

#[test]
fn items_added_from_several_connections_at_once_get_every_number_once() {
    let s = store();
    // the repository first, so that the threads race only for numbers
    add(&s, "pinrail", "Seed");
    let threads: Vec<_> = (0..4)
        .map(|t| {
            let path = s.path.clone();
            std::thread::spawn(move || {
                let db = Db::open(&path).unwrap();
                (0..5)
                    .map(|i| {
                        db.add_item(&new("pinrail", &format!("T{t} {i}")), &agent(), Utc::now())
                            .unwrap()
                            .unwrap()
                            .key
                            .number
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut numbers: Vec<i64> = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    numbers.sort();
    assert_eq!(numbers, (2..=21).collect::<Vec<_>>());
}

// --- naming an item

#[test]
fn an_item_is_found_by_its_key_its_number_in_a_repository_or_its_id() {
    let s = store();
    add(&s, "pinrail", "One");
    add(&s, "pinrail", "Two");
    let id = get(&s, "PIN-2").id;
    for (item, repo) in [
        (r("PIN-2"), None),
        (r("2"), Some("pinrail")),
        (r("#2"), Some("pinrail")),
        (ItemRef::Id(id.clone()), None),
    ] {
        assert_eq!(s.db.item(&item, repo).unwrap().unwrap().id, id);
    }
}

#[test]
fn an_unknown_item_and_a_number_without_a_repository_are_refused() {
    let s = store();
    add(&s, "pinrail", "One");
    assert_eq!(
        s.db.item(&r("PIN-9"), None).unwrap().unwrap_err(),
        Refused::NoItem("PIN-9".into())
    );
    assert_eq!(
        s.db.item(&r("ZZ-1"), None).unwrap().unwrap_err(),
        Refused::NoItem("ZZ-1".into())
    );
    assert_eq!(
        s.db.item(&r("1"), Some("elsewhere")).unwrap().unwrap_err(),
        Refused::NoItem("1".into())
    );
    assert_eq!(
        s.db.item(&r("1"), None).unwrap().unwrap_err(),
        Refused::NumberWithoutRepo(1)
    );
    let refused = s.db.item(&r("1"), None).unwrap().unwrap_err().to_string();
    assert!(refused.contains("PIN-1"), "{refused}");
}

// --- prefixes

#[test]
fn a_changed_prefix_keeps_the_old_keys_working() {
    let s = store();
    add(&s, "forgeplane/pinrail", "One");
    s.db.set_repo_prefix("forgeplane/pinrail", "PR")
        .unwrap()
        .unwrap();
    assert_eq!(get(&s, "PR-1").title, "One");
    assert_eq!(get(&s, "PIN-1").key.to_string(), "PR-1");
    assert_eq!(add(&s, "forgeplane/pinrail", "Two"), "PR-2");
    // and back again: the old prefix is its own to take
    s.db.set_repo_prefix("forgeplane/pinrail", "PIN")
        .unwrap()
        .unwrap();
    assert_eq!(get(&s, "PR-2").key.to_string(), "PIN-2");
}

#[test]
fn a_prefix_another_repository_has_or_had_is_refused() {
    let s = store();
    add(&s, "pinrail", "One");
    add(&s, "acme/api", "Two");
    assert_eq!(
        s.db.set_repo_prefix("acme/api", "PIN").unwrap(),
        Err(Refused::PrefixTaken("PIN".into()))
    );
    s.db.set_repo_prefix("pinrail", "PR").unwrap().unwrap();
    // PIN still finds pinrail's items, so it stays pinrail's
    assert_eq!(
        s.db.set_repo_prefix("acme/api", "PIN").unwrap(),
        Err(Refused::PrefixTaken("PIN".into()))
    );
    // and a new repository's derived prefix passes it by
    assert_eq!(add(&s, "pinrail-site", "Three"), "PS-1");
    assert_eq!(add(&s, "acme/pinrail", "Four"), "PINR-1");
}

#[test]
fn an_invalid_prefix_or_an_unknown_repository_is_refused() {
    let s = store();
    add(&s, "pinrail", "One");
    for bad in ["P", "pin", "PINRAI", "P1", ""] {
        assert_eq!(
            s.db.set_repo_prefix("pinrail", bad).unwrap(),
            Err(Refused::PrefixInvalid(bad.into()))
        );
    }
    assert_eq!(
        s.db.set_repo_prefix("nowhere", "NW").unwrap(),
        Err(Refused::NoRepo("nowhere".into()))
    );
}

// --- moving

#[test]
fn a_moved_item_takes_the_next_number_there_and_keeps_its_old_key() {
    let s = store();
    add(&s, "pinrail", "Parent");
    add(&s, "pinrail", "Moving");
    add(&s, "pinrail", "Needed");
    add(&s, "pinrail-plugins", "There");
    under(&s, "PIN-2", "PIN-1").unwrap();
    depend(&s, "PIN-2", "PIN-3").unwrap();
    let child = add(&s, "pinrail", "Child");
    under(&s, &child, "PIN-2").unwrap();

    let moved = move_to(&s, "PIN-2", "pinrail-plugins");
    assert_eq!(moved.key.to_string(), "PP-2");
    assert_eq!(moved.repo, "pinrail-plugins");
    assert_eq!(get(&s, "PIN-2").key.to_string(), "PP-2");
    // its parent, its dependency and its sub-item stay
    assert_eq!(moved.parent.as_deref(), Some("PIN-1"));
    assert_eq!(moved.blocked_by, ["PIN-3"]);
    assert_eq!(get(&s, &child).parent.as_deref(), Some("PP-2"));
    // its old number is not given again
    assert_eq!(add(&s, "pinrail", "Next"), "PIN-5");
}

#[test]
fn an_item_moved_away_and_back_keeps_every_key_it_had() {
    let s = store();
    add(&s, "pinrail", "First");
    add(&s, "pinrail", "Moving");
    assert_eq!(
        move_to(&s, "PIN-2", "pinrail-plugins").key.to_string(),
        "PP-1"
    );
    assert_eq!(move_to(&s, "PP-1", "pinrail").key.to_string(), "PIN-3");
    for key in ["PIN-2", "PP-1", "PIN-3"] {
        assert_eq!(get(&s, key).key.to_string(), "PIN-3", "{key}");
    }
}

// --- the tree

#[test]
fn an_item_goes_under_a_parent_and_out_again() {
    let s = store();
    add(&s, "pinrail", "Parent");
    add(&s, "pinrail", "Child");
    under(&s, "PIN-2", "PIN-1").unwrap();
    assert_eq!(get(&s, "PIN-2").parent.as_deref(), Some("PIN-1"));
    let parent = get(&s, "PIN-1");
    assert_eq!((parent.sub_items, parent.sub_items_closed), (1, 0));
    edit(
        &s,
        "PIN-2",
        ItemChange {
            parent: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(get(&s, "PIN-2").parent, None);
    assert_eq!(get(&s, "PIN-1").sub_items, 0);
}

#[test]
fn an_item_cannot_be_its_own_parent_nor_under_its_own_sub_item() {
    let s = store();
    for t in ["A", "B", "C", "D"] {
        add(&s, "pinrail", t);
    }
    assert_eq!(under(&s, "PIN-1", "PIN-1"), Err(cycle(&["PIN-1", "PIN-1"])));
    under(&s, "PIN-2", "PIN-1").unwrap();
    under(&s, "PIN-3", "PIN-2").unwrap();
    under(&s, "PIN-4", "PIN-3").unwrap();
    // PIN-1 under PIN-4 would make it wait for itself, four levels round
    assert_eq!(
        under(&s, "PIN-1", "PIN-4"),
        Err(cycle(&["PIN-4", "PIN-1", "PIN-2", "PIN-3", "PIN-4"]))
    );
    let message = under(&s, "PIN-1", "PIN-4").unwrap_err().to_string();
    assert_eq!(
        message,
        "this would make PIN-4 wait for itself: PIN-4 → PIN-1 → PIN-2 → PIN-3 → PIN-4"
    );
}

#[test]
fn deleting_an_item_deletes_its_sub_items_at_any_depth() {
    let s = store();
    for t in ["A", "B", "C", "D"] {
        add(&s, "pinrail", t);
    }
    under(&s, "PIN-2", "PIN-1").unwrap();
    under(&s, "PIN-3", "PIN-2").unwrap();
    assert_eq!(s.db.delete_item(&r("PIN-1"), None).unwrap(), Ok(3));
    assert_eq!(list(&s, &[]), ["PIN-4"]);
}

// --- dependencies

#[test]
fn an_item_depends_on_others_and_no_longer() {
    let s = store();
    for t in ["A", "B", "C"] {
        add(&s, "pinrail", t);
    }
    depend(&s, "PIN-1", "PIN-3").unwrap();
    depend(&s, "PIN-1", "PIN-2").unwrap();
    // depending twice on one item is one dependency
    depend(&s, "PIN-1", "PIN-2").unwrap();
    assert_eq!(get(&s, "PIN-1").blocked_by, ["PIN-2", "PIN-3"]);
    let (on, blocks) = s.db.relations(&get(&s, "PIN-1").id).unwrap();
    assert_eq!(
        on.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["PIN-2", "PIN-3"]
    );
    assert!(blocks.is_empty());
    let (_, blocks) = s.db.relations(&get(&s, "PIN-2").id).unwrap();
    assert_eq!(blocks[0].key, "PIN-1");
    assert_eq!(blocks[0].title, "A");

    edit(
        &s,
        "PIN-1",
        ItemChange {
            no_depends_on: vec![r("PIN-3")],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(get(&s, "PIN-1").blocked_by, ["PIN-2"]);
}

#[test]
fn a_dependency_on_itself_or_round_a_cycle_is_refused() {
    let s = store();
    for t in ["A", "B", "C", "D"] {
        add(&s, "pinrail", t);
    }
    assert_eq!(
        depend(&s, "PIN-1", "PIN-1"),
        Err(cycle(&["PIN-1", "PIN-1"]))
    );
    depend(&s, "PIN-1", "PIN-2").unwrap();
    assert_eq!(
        depend(&s, "PIN-2", "PIN-1"),
        Err(cycle(&["PIN-2", "PIN-1", "PIN-2"]))
    );
    depend(&s, "PIN-2", "PIN-3").unwrap();
    depend(&s, "PIN-3", "PIN-4").unwrap();
    assert_eq!(
        depend(&s, "PIN-4", "PIN-1"),
        Err(cycle(&["PIN-4", "PIN-1", "PIN-2", "PIN-3", "PIN-4"]))
    );
    // nothing of a refused change is kept
    assert!(get(&s, "PIN-4").blocked_by.is_empty());
}

#[test]
fn a_sub_item_cannot_depend_on_an_item_it_is_part_of() {
    let s = store();
    for t in ["Parent", "Child", "Grandchild", "Elsewhere"] {
        add(&s, "pinrail", t);
    }
    under(&s, "PIN-2", "PIN-1").unwrap();
    under(&s, "PIN-3", "PIN-2").unwrap();
    // the parent waits for its sub-items, so they would wait for each other
    assert_eq!(
        depend(&s, "PIN-2", "PIN-1"),
        Err(cycle(&["PIN-2", "PIN-1", "PIN-2"]))
    );
    assert_eq!(
        depend(&s, "PIN-3", "PIN-1"),
        Err(cycle(&["PIN-3", "PIN-1", "PIN-2", "PIN-3"]))
    );
    // nor through another item: PIN-4 depends on PIN-1, PIN-3 on PIN-4
    depend(&s, "PIN-4", "PIN-1").unwrap();
    assert_eq!(
        depend(&s, "PIN-3", "PIN-4"),
        Err(cycle(&["PIN-3", "PIN-4", "PIN-1", "PIN-2", "PIN-3"]))
    );
    // and an item cannot go under an item it depends on
    add(&s, "pinrail", "Fifth");
    depend(&s, "PIN-5", "PIN-4").unwrap();
    assert_eq!(
        under(&s, "PIN-5", "PIN-4"),
        Err(cycle(&["PIN-4", "PIN-5", "PIN-4"]))
    );
    // a parent may depend on its own sub-item: it waits for it anyway
    depend(&s, "PIN-1", "PIN-3").unwrap();
}

#[test]
fn a_dependency_may_cross_repositories_and_parents() {
    let s = store();
    add(&s, "pinrail", "App");
    add(&s, "pinrail-plugins", "Plugin");
    add(&s, "pinrail", "Other parent");
    under(&s, "PIN-1", "PIN-2").unwrap();
    depend(&s, "PIN-1", "PP-1").unwrap();
    assert_eq!(get(&s, "PIN-1").blocked_by, ["PP-1"]);
}

#[test]
fn deleting_an_item_removes_the_dependencies_on_it() {
    let s = store();
    add(&s, "pinrail", "A");
    add(&s, "pinrail", "B");
    depend(&s, "PIN-1", "PIN-2").unwrap();
    s.db.delete_item(&r("PIN-2"), None).unwrap().unwrap();
    assert!(get(&s, "PIN-1").blocked_by.is_empty());
    assert!(get(&s, "PIN-1").ready);
}

// --- blocked and ready, one case each

fn two(s: &Store) {
    add(s, "pinrail", "Item");
    add(s, "pinrail", "Other");
}

#[test]
fn an_open_item_with_no_relations_is_ready() {
    let s = store();
    add(&s, "pinrail", "Alone");
    let item = get(&s, "PIN-1");
    assert!(item.ready && !item.blocked());
}

#[test]
fn an_open_dependency_blocks() {
    let s = store();
    two(&s);
    depend(&s, "PIN-1", "PIN-2").unwrap();
    let item = get(&s, "PIN-1");
    assert!(item.blocked() && !item.ready);
}

#[test]
fn a_dependency_in_progress_blocks() {
    let s = store();
    two(&s);
    depend(&s, "PIN-1", "PIN-2").unwrap();
    status(&s, "PIN-2", Status::InProgress);
    assert!(get(&s, "PIN-1").blocked());
}

#[test]
fn a_done_dependency_does_not_block() {
    let s = store();
    two(&s);
    depend(&s, "PIN-1", "PIN-2").unwrap();
    status(&s, "PIN-2", Status::Done);
    let item = get(&s, "PIN-1");
    assert!(item.ready && !item.blocked());
}

#[test]
fn a_cancelled_dependency_does_not_block() {
    let s = store();
    two(&s);
    depend(&s, "PIN-1", "PIN-2").unwrap();
    status(&s, "PIN-2", Status::Cancelled);
    let item = get(&s, "PIN-1");
    assert!(item.ready && !item.blocked());
    // and it is still listed among what the item depends on
    let (on, _) = s.db.relations(&item.id).unwrap();
    assert_eq!(on[0].status, Status::Cancelled);
}

#[test]
fn one_open_dependency_of_two_blocks() {
    let s = store();
    two(&s);
    add(&s, "pinrail", "Third");
    depend(&s, "PIN-1", "PIN-2").unwrap();
    depend(&s, "PIN-1", "PIN-3").unwrap();
    status(&s, "PIN-2", Status::Done);
    assert_eq!(get(&s, "PIN-1").blocked_by, ["PIN-3"]);
}

#[test]
fn a_parent_with_an_open_sub_item_is_not_ready_nor_blocked() {
    let s = store();
    two(&s);
    under(&s, "PIN-2", "PIN-1").unwrap();
    let parent = get(&s, "PIN-1");
    assert!(!parent.ready && !parent.blocked());
    status(&s, "PIN-2", Status::InProgress);
    assert!(!get(&s, "PIN-1").ready);
}

#[test]
fn a_parent_whose_sub_items_are_closed_is_ready() {
    let s = store();
    two(&s);
    add(&s, "pinrail", "Third");
    under(&s, "PIN-2", "PIN-1").unwrap();
    under(&s, "PIN-3", "PIN-1").unwrap();
    status(&s, "PIN-2", Status::Done);
    status(&s, "PIN-3", Status::Cancelled);
    let parent = get(&s, "PIN-1");
    assert!(parent.ready);
    assert_eq!((parent.sub_items, parent.sub_items_closed), (2, 2));
}

#[test]
fn an_item_in_progress_done_or_cancelled_is_not_ready() {
    let s = store();
    for t in ["A", "B", "C"] {
        add(&s, "pinrail", t);
    }
    status(&s, "PIN-1", Status::InProgress);
    status(&s, "PIN-2", Status::Done);
    status(&s, "PIN-3", Status::Cancelled);
    for key in ["PIN-1", "PIN-2", "PIN-3"] {
        assert!(!get(&s, key).ready, "{key}");
    }
}

#[test]
fn a_ready_sub_item_of_a_blocked_parent_is_ready() {
    let s = store();
    for t in ["Parent", "Child", "Blocker"] {
        add(&s, "pinrail", t);
    }
    under(&s, "PIN-2", "PIN-1").unwrap();
    depend(&s, "PIN-1", "PIN-3").unwrap();
    assert!(get(&s, "PIN-1").blocked());
    assert!(get(&s, "PIN-2").ready);
}

// --- status

#[test]
fn closing_an_item_records_when_and_opening_it_clears_that() {
    let s = store();
    add(&s, "pinrail", "A");
    status(&s, "PIN-1", Status::Done);
    let closed = get(&s, "PIN-1").closed_at.unwrap();
    // cancelling a done item keeps when it first closed
    status(&s, "PIN-1", Status::Cancelled);
    assert_eq!(get(&s, "PIN-1").closed_at, Some(closed));
    status(&s, "PIN-1", Status::Open);
    assert_eq!(get(&s, "PIN-1").closed_at, None);
}

// --- listing

#[test]
fn a_list_shows_the_items_not_closed_unless_asked() {
    let s = store();
    for t in ["Open", "Going", "Done", "Dropped"] {
        add(&s, "pinrail", t);
    }
    status(&s, "PIN-2", Status::InProgress);
    status(&s, "PIN-3", Status::Done);
    status(&s, "PIN-4", Status::Cancelled);
    assert_eq!(list(&s, &[]), ["PIN-1", "PIN-2"]);
    assert_eq!(list(&s, &[State::Done]), ["PIN-3"]);
    assert_eq!(
        list(&s, &[State::Cancelled, State::Open]),
        ["PIN-1", "PIN-4"]
    );
}

#[test]
fn a_list_of_ready_or_blocked_items_derives_them() {
    let s = store();
    for t in [
        "Ready",
        "Blocked",
        "Blocker",
        "Parent",
        "Child",
        "Blocked going",
    ] {
        add(&s, "pinrail", t);
    }
    depend(&s, "PIN-2", "PIN-3").unwrap();
    under(&s, "PIN-5", "PIN-4").unwrap();
    depend(&s, "PIN-6", "PIN-3").unwrap();
    status(&s, "PIN-6", Status::InProgress);
    assert_eq!(list(&s, &[State::Ready]), ["PIN-1", "PIN-3", "PIN-5"]);
    assert_eq!(list(&s, &[State::Blocked]), ["PIN-2", "PIN-6"]);
    assert_eq!(
        list(&s, &[State::Ready, State::Blocked]),
        ["PIN-1", "PIN-2", "PIN-3", "PIN-5", "PIN-6"]
    );
}

#[test]
fn a_list_narrows_by_repository_project_parent_author_and_words() {
    let s = store();
    add(&s, "pinrail", "Theme loader");
    add(&s, "pinrail", "Theme picker");
    add(&s, "pinrail-plugins", "Catalog search");
    let mut person = new("pinrail", "Release notes");
    person.body = "Write the notes for the theme work".into();
    s.db.add_item(&person, &Actor::Person, Utc::now())
        .unwrap()
        .unwrap();
    under(&s, "PIN-2", "PIN-1").unwrap();

    let by = |filters: ItemFilters| keys(&s.db.items(&filters).unwrap());
    assert_eq!(
        by(ItemFilters {
            repo: Some("pinrail-plugins".into()),
            ..Default::default()
        }),
        ["PP-1"]
    );
    assert_eq!(
        by(ItemFilters {
            parent: Some(get(&s, "PIN-1").id),
            ..Default::default()
        }),
        ["PIN-2"]
    );
    assert_eq!(
        by(ItemFilters {
            created_by: Some("person".into()),
            ..Default::default()
        }),
        ["PIN-3"]
    );
    // every word, in the title or the body, whatever the case
    assert_eq!(
        by(ItemFilters {
            text: Some("THEME".into()),
            ..Default::default()
        }),
        ["PIN-1", "PIN-2", "PIN-3"]
    );
    assert_eq!(
        by(ItemFilters {
            text: Some("theme picker".into()),
            ..Default::default()
        }),
        ["PIN-2"]
    );
    // a % or _ is a character, not a wildcard
    assert!(
        by(ItemFilters {
            text: Some("%".into()),
            ..Default::default()
        })
        .is_empty()
    );
    // combined
    assert_eq!(
        by(ItemFilters {
            repo: Some("pinrail".into()),
            text: Some("theme".into()),
            created_by: Some("claude-code".into()),
            ..Default::default()
        }),
        ["PIN-1", "PIN-2"]
    );
}

#[test]
fn a_list_narrows_to_a_projects_repositories() {
    let s = store();
    add(&s, "pinrail", "App");
    add(&s, "pinrail-plugins", "Plugin");
    add(&s, "acme/api", "Elsewhere");
    let conn = rusqlite::Connection::open(&s.path).unwrap();
    conn.execute_batch(
        "INSERT INTO projects (id, name, created_at) VALUES ('p_1', 'Pinrail', '2026-11-06T00:00:00Z');
         UPDATE repos SET project_id = 'p_1' WHERE name IN ('pinrail', 'pinrail-plugins');",
    )
    .unwrap();
    assert_eq!(
        keys(
            &s.db
                .items(&ItemFilters {
                    project: Some("p_1".into()),
                    ..Default::default()
                })
                .unwrap()
        ),
        ["PIN-1", "PP-1"]
    );
}

// --- adding with relations

#[test]
fn an_item_is_added_under_a_parent_and_depending_on_others() {
    let s = store();
    add(&s, "pinrail", "Parent");
    add(&s, "pinrail", "Needed");
    let mut item = new("pinrail", "Child");
    item.parent = Some(r("1"));
    item.depends_on = vec![r("PIN-2")];
    item.link = Some("https://github.com/forgeplane/pinrail/issues/7".into());
    let added = s.db.add_item(&item, &agent(), Utc::now()).unwrap().unwrap();
    assert_eq!(added.key.to_string(), "PIN-3");
    assert_eq!(added.parent.as_deref(), Some("PIN-1"));
    assert_eq!(added.blocked_by, ["PIN-2"]);
    assert_eq!(
        added.link.as_deref(),
        Some("https://github.com/forgeplane/pinrail/issues/7")
    );
}

#[test]
fn an_item_whose_parent_or_dependency_is_unknown_is_not_added() {
    let s = store();
    let mut item = new("pinrail", "Orphan");
    item.parent = Some(r("PIN-9"));
    assert_eq!(
        s.db.add_item(&item, &agent(), Utc::now()).unwrap(),
        Err(Refused::NoItem("PIN-9".into()))
    );
    assert!(list(&s, &[]).is_empty());
    // nor did it use a number
    assert_eq!(add(&s, "pinrail", "First"), "PIN-1");
}

// --- breakdowns

fn piece(r: &str, title: &str, parent: Option<&str>, depends_on: &[&str]) -> BreakdownItem {
    BreakdownItem {
        r#ref: r.into(),
        title: title.into(),
        body: String::new(),
        parent: parent.map(str::to_string),
        depends_on: depends_on.iter().map(|d| d.to_string()).collect(),
        link: None,
    }
}

fn breakdown(items: Vec<BreakdownItem>) -> Breakdown {
    Breakdown {
        repo: "forgeplane/pinrail".into(),
        parent: None,
        items,
    }
}

#[test]
fn a_breakdown_is_added_whole_with_its_refs_resolved() {
    let s = store();
    add(&s, "forgeplane/pinrail", "Settings schema");
    let added =
        s.db.add_breakdown(
            &breakdown(vec![
                piece("themes", "Custom themes", None, &[]),
                piece("format", "Theme file format", Some("themes"), &[]),
                piece("load", "Load themes", Some("themes"), &["format"]),
                piece("picker", "Theme picker", Some("themes"), &["load", "PIN-1"]),
            ]),
            &agent(),
            Utc::now(),
        )
        .unwrap()
        .unwrap();
    let pairs: Vec<(String, String)> = added
        .iter()
        .map(|(r, i)| (r.clone(), i.key.to_string()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("themes".into(), "PIN-2".into()),
            ("format".into(), "PIN-3".into()),
            ("load".into(), "PIN-4".into()),
            ("picker".into(), "PIN-5".into()),
        ]
    );
    assert_eq!(get(&s, "PIN-4").parent.as_deref(), Some("PIN-2"));
    assert_eq!(get(&s, "PIN-4").blocked_by, ["PIN-3"]);
    assert_eq!(get(&s, "PIN-5").blocked_by, ["PIN-1", "PIN-4"]);
    assert_eq!(list(&s, &[State::Ready]), ["PIN-1", "PIN-3"]);
}

#[test]
fn a_breakdown_goes_under_an_existing_item() {
    let s = store();
    add(&s, "forgeplane/pinrail", "Release");
    let mut b = breakdown(vec![
        piece("notes", "Notes", None, &[]),
        piece("tag", "Tag", None, &["notes"]),
        piece("sub", "Sub", Some("tag"), &[]),
    ]);
    b.parent = Some(r("PIN-1"));
    s.db.add_breakdown(&b, &agent(), Utc::now())
        .unwrap()
        .unwrap();
    assert_eq!(get(&s, "PIN-2").parent.as_deref(), Some("PIN-1"));
    assert_eq!(get(&s, "PIN-3").parent.as_deref(), Some("PIN-1"));
    // an item with a parent of its own keeps it
    assert_eq!(get(&s, "PIN-4").parent.as_deref(), Some("PIN-3"));
}

#[test]
fn a_breakdown_with_a_fault_adds_nothing_and_uses_no_number() {
    let s = store();
    add(&s, "forgeplane/pinrail", "Existing");
    let cases = [
        (
            vec![piece("a", "A", None, &[]), piece("a", "A again", None, &[])],
            Refused::DuplicateRef("a".into()),
        ),
        (
            vec![piece("a", "A", None, &["nope"])],
            Refused::UnknownRef("nope".into()),
        ),
        (
            vec![piece("a", "A", Some("PIN-9"), &[])],
            Refused::NoItem("PIN-9".into()),
        ),
        (
            vec![piece("a", "A", None, &["b"]), piece("b", "B", None, &["a"])],
            cycle(&["PIN-3", "PIN-2", "PIN-3"]),
        ),
        (
            vec![
                piece("a", "A", None, &[]),
                piece("b", "B", Some("a"), &["a"]),
            ],
            cycle(&["PIN-3", "PIN-2", "PIN-3"]),
        ),
    ];
    for (items, refused) in cases {
        assert_eq!(
            s.db.add_breakdown(&breakdown(items), &agent(), Utc::now())
                .unwrap(),
            Err(refused)
        );
        assert_eq!(list(&s, &[]), ["PIN-1"]);
    }
    assert_eq!(add(&s, "forgeplane/pinrail", "Next"), "PIN-2");
}

#[test]
fn a_breakdown_cannot_close_a_cycle_through_existing_items() {
    let s = store();
    add(&s, "forgeplane/pinrail", "Parent");
    add(&s, "forgeplane/pinrail", "Waits for the parent");
    depend(&s, "PIN-2", "PIN-1").unwrap();
    // under PIN-1 and depending on PIN-2, which waits for PIN-1
    let b = breakdown(vec![piece("a", "A", Some("PIN-1"), &["PIN-2"])]);
    assert_eq!(
        s.db.add_breakdown(&b, &agent(), Utc::now()).unwrap(),
        Err(cycle(&["PIN-3", "PIN-2", "PIN-1", "PIN-3"]))
    );
    assert_eq!(list(&s, &[]), ["PIN-1", "PIN-2"]);
}
