//! Searching work items by their words: each phrasing is matched against
//! the full-text index of the items' titles, bodies and comments and
//! ranked by BM25, and the rankings of several phrasings are merged by
//! reciprocal rank fusion.

use std::collections::HashMap;

use rusqlite::params;

use super::Db;
use super::items::{ItemFilters, State, WorkItem, searched_number};

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

/// One phrasing's matches, best first: the item's id, its score, and
/// where it matched with the words around it.
type Ranked = Vec<(String, &'static str, Option<String>)>;

impl Db {
    /// The items that `search` finds, best first, at most `search.limit`.
    pub fn search_items(&self, search: &ItemSearch) -> rusqlite::Result<Vec<Found>> {
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
        let among: HashMap<String, WorkItem> = self
            .items(&filters)?
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect();

        // each phrasing's ranking, fused: an item found by several comes first
        let mut fused: HashMap<String, (f64, usize, &'static str, Option<String>)> = HashMap::new();
        for phrasing in &search.phrasings {
            let ranked = self.rank(phrasing, &among)?;
            for (place, (id, matched, snippet)) in ranked.into_iter().enumerate() {
                let entry = fused.entry(id).or_insert((0.0, usize::MAX, matched, None));
                entry.0 += 1.0 / (FUSION_K + place as f64 + 1.0);
                // where it matched in the phrasing that ranked it best
                if place < entry.1 {
                    entry.1 = place;
                    entry.2 = matched;
                    entry.3 = snippet;
                }
            }
        }
        let mut found: Vec<Found> = fused
            .into_iter()
            .filter_map(|(id, (score, _, matched, snippet))| {
                among.get(&id).map(|item| Found {
                    item: item.clone(),
                    score,
                    matched,
                    snippet,
                })
            })
            .collect();
        found.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| b.item.updated_at.cmp(&a.item.updated_at))
        });
        found.truncate(search.limit);
        Ok(found)
    }

    /// A phrasing's matches among `among`, best first: an item it names by
    /// its key or number, then the full-text matches by BM25, an item not
    /// closed counting a little more.
    fn rank(&self, phrasing: &str, among: &HashMap<String, WorkItem>) -> rusqlite::Result<Ranked> {
        let mut ranked: Ranked = Vec::new();
        for word in phrasing.split_whitespace() {
            if let Some((prefix, number)) = searched_number(word) {
                ranked.extend(
                    among
                        .values()
                        .filter(|item| {
                            item.key.number == number
                                && prefix.as_ref().is_none_or(|p| *p == item.key.prefix)
                        })
                        .map(|item| (item.id.clone(), "key", None)),
                );
            }
        }
        let Some(query) = match_query(phrasing) else {
            return Ok(ranked);
        };
        let (title, body, comments) = WEIGHTS;
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT r.item_id, bm25(work_item_search, {title}, {body}, {comments}),
               snippet(work_item_search, 0, ?2, ?3, '…', 12),
               snippet(work_item_search, 1, ?2, ?3, '…', 12),
               snippet(work_item_search, 2, ?2, ?3, '…', 12)
             FROM work_item_search
             JOIN work_item_search_rows r ON r.row = work_item_search.rowid
             WHERE work_item_search MATCH ?1
             ORDER BY 2 LIMIT {}",
            DEPTH * 2
        ))?;
        let rows = stmt.query_map(params![query, OPEN_MARK, CLOSE_MARK], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                [
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ],
            ))
        })?;
        let mut scored: Vec<(String, f64, &'static str, Option<String>)> = Vec::new();
        for row in rows {
            let (id, bm25, snippets) = row?;
            let Some(item) = among.get(&id) else {
                continue;
            };
            // BM25 is negative here, lower being better
            let open = !matches!(item.status.as_str(), "done" | "cancelled");
            let score = if open { bm25 * OPEN_BOOST } else { bm25 };
            let (matched, snippet) = snippets
                .iter()
                .zip(COLUMNS)
                .find_map(|(s, column)| {
                    s.as_ref()
                        .filter(|s| s.contains(OPEN_MARK))
                        .map(|s| (column, s))
                })
                .map(|(column, s)| (column, Some(readable(s))))
                .unwrap_or(("title", None));
            scored.push((id, score, matched, snippet));
        }
        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (id, _, matched, snippet) in scored {
            if !ranked.iter().any(|(r, _, _)| *r == id) {
                ranked.push((id, matched, snippet));
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
