//! Searching work items by their words: each phrasing is matched against
//! the full-text index of the items' titles, bodies and comments and
//! ranked by BM25, and the rankings of several phrasings are merged by
//! reciprocal rank fusion.

use std::collections::HashMap;

use rusqlite::OptionalExtension;

use super::Db;
use super::items::{ItemFilters, State, WorkItem, searched_number, states_sql};

/// What to search for, and among which items.
#[derive(Debug, Clone, Default)]
pub struct ItemSearch {
    /// one or more ways of saying what is looked for
    pub phrasings: Vec<String>,
    /// the items searched: by default every status, in every repository
    pub filters: ItemFilters,
    pub limit: usize,
}

/// An item a search found, best first.
#[derive(Debug, Clone)]
pub struct Found {
    pub item: WorkItem,
    /// the fused score: higher is better
    pub score: f64,
    /// where it matched best: `key`, `title`, `body` or `comment`
    pub matched: &'static str,
    /// the words around the match, with each matched word in brackets
    pub snippet: Option<String>,
}

/// How much a match in the title, the body and the comments counts, as
/// measured by `tests/search_quality.rs`: comments hold much of what is
/// found and decided, and count as much as the body.
const WEIGHTS: (f64, f64, f64) = (3.0, 1.0, 1.0);
/// How much an item not closed counts above a closed one.
const OPEN_BOOST: f64 = 1.15;
/// The constant of reciprocal rank fusion.
const FUSION_K: f64 = 60.0;
/// How many of a phrasing's matches are fused.
const DEPTH: usize = 100;
/// The column names, in the index's order, as a result names them.
const COLUMNS: [&str; 3] = ["title", "body", "comment"];
/// Marks a matched word in a snippet while it is read, as no item's text
/// holds these.
const OPEN_MARK: &str = "\u{1}";
const CLOSE_MARK: &str = "\u{2}";

/// Words too common to tell items apart.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "is", "it", "of", "on",
    "or", "that", "the", "this", "to", "was", "when", "with",
];

/// The FTS5 query for a phrasing: any of its words, each quoted so that
/// nothing in it is read as FTS5 syntax, and a quoted run of words kept as
/// one phrase. Common words are left out unless nothing else is left.
/// None when it has no word.
fn match_query(phrasing: &str) -> Option<String> {
    let mut terms: Vec<(String, bool)> = Vec::new();
    for (i, part) in phrasing.split('"').enumerate() {
        let quoted = i % 2 == 1;
        if quoted {
            terms.push((part.to_string(), true));
        } else {
            terms.extend(part.split_whitespace().map(|w| (w.to_string(), false)));
        }
    }
    let worded: Vec<(String, bool)> = terms
        .into_iter()
        .filter(|(t, _)| t.chars().any(char::is_alphanumeric))
        .collect();
    let telling: Vec<&(String, bool)> = worded
        .iter()
        .filter(|(t, phrase)| *phrase || !STOP_WORDS.contains(&t.to_lowercase().as_str()))
        .collect();
    let kept: Vec<&(String, bool)> = if telling.is_empty() {
        worded.iter().collect()
    } else {
        telling
    };
    if kept.is_empty() {
        return None;
    }
    Some(
        kept.iter()
            .map(|(t, _)| format!("\"{}\"", t.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// One phrasing's matches, best first: the item's id, and whether the
/// phrasing named it by its key or number.
type Ranked = Vec<(String, bool)>;

/// The SQL that keeps the items, as `i` in repository `g`, that `filters`
/// names by status, repository and project, with its arguments numbered
/// from `first`.
fn kept(filters: &ItemFilters, first: usize) -> (String, Vec<String>) {
    let states = if filters.states.is_empty() {
        vec![
            State::Open,
            State::InProgress,
            State::OnHold,
            State::Done,
            State::Cancelled,
        ]
    } else {
        filters.states.clone()
    };
    let mut wheres = vec![states_sql(&states)];
    let mut args = Vec::new();
    if let Some(repo) = &filters.repo {
        args.push(repo.clone());
        wheres.push(format!("g.name = ?{}", first + args.len() - 1));
    }
    if let Some(project) = &filters.project {
        args.push(project.clone());
        wheres.push(format!("g.project_id = ?{}", first + args.len() - 1));
    }
    (wheres.join(" AND "), args)
}

impl Db {
    /// The items that `search` finds, best first, at most `search.limit`.
    /// Its filters' statuses (by default every status), repository and
    /// project narrow the search itself; any other filter only what is
    /// returned.
    pub fn search_items(&self, search: &ItemSearch) -> rusqlite::Result<Vec<Found>> {
        // each phrasing's ranking, fused: an item found by several comes
        // first; each keeps the phrasing that ranked it best
        let mut fused: HashMap<String, (f64, usize, usize, bool)> = HashMap::new();
        for (n, phrasing) in search.phrasings.iter().enumerate() {
            let ranked = self.rank(phrasing, &search.filters)?;
            for (place, (id, by_key)) in ranked.into_iter().enumerate() {
                let entry = fused.entry(id).or_insert((0.0, usize::MAX, n, by_key));
                entry.0 += 1.0 / (FUSION_K + place as f64 + 1.0);
                if place < entry.1 {
                    entry.1 = place;
                    entry.2 = n;
                    entry.3 = by_key;
                }
            }
        }
        let mut order: Vec<(String, (f64, usize, usize, bool))> = fused.into_iter().collect();
        order.sort_by(|a, b| b.1.0.total_cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
        order.truncate(search.limit);

        // only the items returned are read in full
        let mut filters = search.filters.clone();
        if filters.states.is_empty() {
            filters.states = vec![
                State::Open,
                State::InProgress,
                State::OnHold,
                State::Done,
                State::Cancelled,
            ];
        }
        filters.summary = true;
        filters.ids = order.iter().map(|(id, _)| id.clone()).collect();
        if filters.ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut items: HashMap<String, WorkItem> = self
            .items(&filters)?
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect();
        let mut found = Vec::new();
        for (id, (score, _, phrasing, by_key)) in order {
            let Some(item) = items.remove(&id) else {
                continue;
            };
            // where it matched, only for what is returned: a snippet costs
            let (matched, snippet) = if by_key {
                ("key", None)
            } else {
                self.matched(&search.phrasings[phrasing], &id)?
            };
            found.push(Found {
                item,
                score,
                matched,
                snippet,
            });
        }
        Ok(found)
    }

    /// Where `phrasing` matched the item `id` first, of its title, body
    /// and comments, with the words around the match.
    fn matched(
        &self,
        phrasing: &str,
        id: &str,
    ) -> rusqlite::Result<(&'static str, Option<String>)> {
        let Some(query) = match_query(phrasing) else {
            return Ok(("title", None));
        };
        let conn = self.read();
        let snippets = conn
            .query_row(
                "SELECT snippet(work_item_search, 0, ?3, ?4, '…', 12),
                   snippet(work_item_search, 1, ?3, ?4, '…', 12),
                   snippet(work_item_search, 2, ?3, ?4, '…', 12)
                 FROM work_item_search
                 WHERE work_item_search MATCH ?1
                   AND rowid = (SELECT row FROM work_item_search_rows WHERE item_id = ?2)",
                rusqlite::params![query, id, OPEN_MARK, CLOSE_MARK],
                |r| {
                    Ok([
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ])
                },
            )
            .optional()?
            .unwrap_or_default();
        Ok(snippets
            .iter()
            .zip(COLUMNS)
            .find_map(|(s, column)| {
                s.as_ref()
                    .filter(|s| s.contains(OPEN_MARK))
                    .map(|s| (column, Some(readable(s))))
            })
            .unwrap_or(("title", None)))
    }

    /// A phrasing's matches among the items `filters` keeps, best first: an
    /// item it names by its key or number, then the full-text matches by
    /// BM25, an item not closed counting a little more.
    fn rank(&self, phrasing: &str, filters: &ItemFilters) -> rusqlite::Result<Ranked> {
        let conn = self.read();
        let mut ranked: Ranked = Vec::new();
        for word in phrasing.split_whitespace() {
            let Some((prefix, number)) = searched_number(word) else {
                continue;
            };
            let (keep, mut args) = kept(filters, 3);
            args.insert(0, number.to_string());
            args.insert(1, prefix.unwrap_or_default());
            let mut stmt = conn.prepare(&format!(
                "SELECT i.id FROM work_items i JOIN repos g ON g.id = i.repo_id
                  WHERE i.number = ?1 AND (?2 = '' OR g.prefix = ?2) AND {keep}"
            ))?;
            let ids = stmt.query_map(rusqlite::params_from_iter(&args), |r| r.get(0))?;
            for id in ids {
                ranked.push((id?, true));
            }
        }
        let Some(query) = match_query(phrasing) else {
            return Ok(ranked);
        };
        let (title, body, comments) = WEIGHTS;
        let (keep, filter_args) = kept(filters, 2);
        let mut args = vec![query];
        args.extend(filter_args);
        // BM25 is negative here, lower being better
        let mut stmt = conn.prepare(&format!(
            "SELECT r.item_id FROM work_item_search
             JOIN work_item_search_rows r ON r.row = work_item_search.rowid
             JOIN work_items i ON i.id = r.item_id
             JOIN repos g ON g.id = i.repo_id
             WHERE work_item_search MATCH ?1 AND {keep}
             ORDER BY bm25(work_item_search, {title}, {body}, {comments})
               * CASE WHEN i.status IN ('done', 'cancelled') THEN 1.0 ELSE {OPEN_BOOST} END
             LIMIT {DEPTH}"
        ))?;
        let ids = stmt.query_map(rusqlite::params_from_iter(&args), |r| r.get::<_, String>(0))?;
        for id in ids {
            let id = id?;
            if !ranked.iter().any(|(r, _)| *r == id) {
                ranked.push((id, false));
            }
        }
        ranked.truncate(DEPTH);
        Ok(ranked)
    }
}

/// A snippet on one line, with each matched word in brackets.
fn readable(snippet: &str) -> String {
    snippet
        .replace(OPEN_MARK, "[")
        .replace(CLOSE_MARK, "]")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::match_query;

    #[test]
    fn a_phrasing_becomes_any_of_its_quoted_words() {
        assert_eq!(
            match_query("slow session").as_deref(),
            Some("\"slow\" OR \"session\"")
        );
        // common words go, unless nothing else is left
        assert_eq!(
            match_query("the size of the header").as_deref(),
            Some("\"size\" OR \"header\"")
        );
        assert_eq!(match_query("the").as_deref(), Some("\"the\""));
        // a quoted run of words stays a phrase, and FTS5 syntax is a word
        assert_eq!(
            match_query("\"find in terminal\" NEAR(x) *").as_deref(),
            Some("\"find in terminal\" OR \"NEAR(x)\"")
        );
        assert_eq!(match_query("  -- ").as_deref(), None);
    }
}
