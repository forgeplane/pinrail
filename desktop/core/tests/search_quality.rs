//! How well searching work items finds the items a person would expect,
//! measured on a copy of a real database. The queries and the items each
//! should find are in `search/queries.json`; the items' texts stay in the
//! database, outside the repository.
//!
//! ```sh
//! PINRAIL_SEARCH_DB=/path/to/a/copy/of/pinrail.db \
//!   cargo test --test search_quality -- --ignored --nocapture
//! ```
//!
//! The database is migrated in place, so give it a copy.

use std::path::PathBuf;

use pinrail_core::db::{Db, ItemFilters, ItemSearch, State};
use serde_json::Value;

/// How many results an agent reads.
const TOP: usize = 10;

/// A way of searching: the keys it finds for a query and the other ways
/// of saying it, best first.
type Search = fn(&Db, &str, &str, &[String]) -> Vec<String>;

/// Today's search: every word in the title or the body, in key order.
fn like(db: &Db, repo: &str, query: &str, _: &[String]) -> Vec<String> {
    let filters = ItemFilters {
        states: vec![
            State::Open,
            State::InProgress,
            State::Done,
            State::Cancelled,
        ],
        repo: Some(repo.to_string()),
        text: Some(query.to_string()),
        summary: true,
        ..Default::default()
    };
    db.items(&filters)
        .unwrap()
        .into_iter()
        .map(|item| item.key.to_string())
        .collect()
}

/// The ranked full-text search, with the query alone.
fn ranked(db: &Db, repo: &str, query: &str, _: &[String]) -> Vec<String> {
    search(db, repo, vec![query.to_string()])
}

/// The ranked full-text search, with the query and the other ways of
/// saying it, as an agent told to search with several phrasings does.
fn phrasings(db: &Db, repo: &str, query: &str, more: &[String]) -> Vec<String> {
    let mut all = vec![query.to_string()];
    all.extend(more.iter().cloned());
    search(db, repo, all)
}

fn search(db: &Db, repo: &str, phrasings: Vec<String>) -> Vec<String> {
    let search = ItemSearch {
        phrasings,
        filters: ItemFilters {
            repo: Some(repo.to_string()),
            ..Default::default()
        },
        limit: 50,
    };
    db.search_items(&search)
        .unwrap()
        .into_iter()
        .map(|found| found.item.key.to_string())
        .collect()
}

#[test]
#[ignore = "needs PINRAIL_SEARCH_DB, a copy of a real database"]
fn search_finds_the_expected_items() {
    let path = PathBuf::from(std::env::var("PINRAIL_SEARCH_DB").expect("PINRAIL_SEARCH_DB"));
    let db = Db::open(&path).unwrap();
    let set: Value = serde_json::from_str(include_str!("search/queries.json")).unwrap();
    let repo = set["repo"].as_str().unwrap();
    let queries = set["queries"].as_array().unwrap();

    let searches: [(&str, Search); 3] =
        [("like", like), ("ranked", ranked), ("phrasings", phrasings)];
    for (name, search) in searches {
        let (mut expected, mut in_top, mut anywhere, mut returned) = (0, 0, 0, 0);
        println!("\n{name}");
        for query in queries {
            let q = query["q"].as_str().unwrap();
            let expect: Vec<&str> = query["expect"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap())
                .collect();
            let more: Vec<String> = query["more"]
                .as_array()
                .map(|m| m.iter().map(|p| p.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            let found = search(&db, repo, q, &more);
            let top = &found[..found.len().min(TOP)];
            let hits = expect
                .iter()
                .filter(|k| top.iter().any(|f| f == *k))
                .count();
            let all = expect
                .iter()
                .filter(|k| found.iter().any(|f| f == *k))
                .count();
            println!(
                "  {hits}/{} in top {TOP}, {all} of {} results  {q}",
                expect.len(),
                found.len()
            );
            expected += expect.len();
            in_top += hits;
            anywhere += all;
            returned += found.len();
        }
        // the time a query takes, once the database is warm
        const ROUNDS: u32 = 20;
        let started = std::time::Instant::now();
        for _ in 0..ROUNDS {
            for query in queries {
                let more: Vec<String> = query["more"]
                    .as_array()
                    .map(|m| m.iter().map(|p| p.as_str().unwrap().to_string()).collect())
                    .unwrap_or_default();
                search(&db, repo, query["q"].as_str().unwrap(), &more);
            }
        }
        let each = started.elapsed() / (ROUNDS * queries.len() as u32);
        println!(
            "{name}: recall@{TOP} {:.2}, recall of all results {:.2}, {:.1} results a query, \
             {:.2} ms a query",
            in_top as f64 / expected as f64,
            anywhere as f64 / expected as f64,
            returned as f64 / queries.len() as f64,
            each.as_secs_f64() * 1000.0
        );
    }
}
