//! The chemical elements a session is named after, with the adjectives of
//! the names: the 118 elements with their place on the periodic table,
//! their category, how rare they are among session names and one sentence
//! about each, and the adjectives by first letter, with a general list
//! for any element.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// How often a session gets an element of this rarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    /// The share of draws that land on this rarity, in percent.
    pub fn weight(self) -> u32 {
        match self {
            Rarity::Common => 60,
            Rarity::Uncommon => 30,
            Rarity::Rare => 9,
            Rarity::Legendary => 1,
        }
    }
}

/// An element, as the periodic table shows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    /// the atomic number, from 1 to 118
    pub number: u32,
    pub symbol: String,
    pub name: String,
    /// such as "noble gas" or "transition metal"
    pub category: String,
    pub rarity: Rarity,
    /// its row and column on the table: rows 1 to 7 are the periods, and
    /// rows 9 and 10 the lanthanides and actinides drawn below
    pub row: u32,
    pub column: u32,
    /// one sentence about it
    pub description: String,
}

#[derive(Deserialize)]
struct Adjectives {
    by_letter: BTreeMap<String, Vec<String>>,
    general: Vec<String>,
}

static ELEMENTS: LazyLock<Vec<Element>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("elements/elements.json"))
        .expect("the element list is valid JSON")
});

static ADJECTIVES: LazyLock<Adjectives> = LazyLock::new(|| {
    serde_json::from_str(include_str!("elements/adjectives.json"))
        .expect("the adjectives are valid JSON")
});

/// The 118 elements, by atomic number.
pub fn all() -> &'static [Element] {
    &ELEMENTS
}

/// The element of an atomic number.
pub fn by_number(number: u32) -> Option<&'static Element> {
    ELEMENTS.get(usize::try_from(number).ok()?.checked_sub(1)?)
}

/// The adjectives that start with the element's first letter, which make
/// an alliterative name such as Happy Hydrogen.
pub fn own_adjectives(element: &Element) -> &'static [String] {
    let letter = element
        .name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    ADJECTIVES
        .by_letter
        .get(&letter)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// The adjectives for any element.
pub fn general_adjectives() -> &'static [String] {
    &ADJECTIVES.general
}

/// The element and the generated name of a session, derived from a hash
/// of its agent and id, so a session always has the same one. The element
/// is picked by its rarity's weight, and the adjective from the element's
/// own and the general list together.
pub fn name_of(agent: &str, id: &str) -> (u32, String) {
    let digest = Sha256::new()
        .chain_update(agent)
        .chain_update([0])
        .chain_update(id)
        .finalize();
    let word = |i: usize| u64::from_le_bytes(digest[i..i + 8].try_into().expect("8 bytes"));
    let element = pick_element(fraction(word(0)));
    let own = own_adjectives(element);
    let general = general_adjectives();
    let at = (word(8) % (own.len() + general.len()) as u64) as usize;
    let adjective = own.get(at).unwrap_or_else(|| &general[at - own.len()]);
    (element.number, format!("{adjective} {}", element.name))
}

/// A number in [0, 1) from the top 53 bits of `word`.
fn fraction(word: u64) -> f64 {
    (word >> 11) as f64 / (1u64 << 53) as f64
}

/// The element at `at`, a number in [0, 1), when each rarity takes its
/// share of [0, 1) and splits it evenly among its elements.
fn pick_element(at: f64) -> &'static Element {
    let count = |rarity: Rarity| all().iter().filter(|e| e.rarity == rarity).count() as f64;
    let mut left = at * 100.0;
    for e in all() {
        left -= f64::from(e.rarity.weight()) / count(e.rarity);
        if left < 0.0 {
            return e;
        }
    }
    all().last().expect("the list has elements")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn the_list_has_every_element_once_in_order_with_its_own_place() {
        let numbers: Vec<u32> = all().iter().map(|e| e.number).collect();
        assert_eq!(numbers, (1..=118).collect::<Vec<_>>());
        let places: HashSet<(u32, u32)> = all().iter().map(|e| (e.row, e.column)).collect();
        assert_eq!(places.len(), 118);
        for e in all() {
            assert!((1..=18).contains(&e.column), "{}", e.name);
            assert!(matches!(e.row, 1..=7 | 9 | 10), "{}", e.name);
        }
        assert_eq!(by_number(1).map(|e| e.symbol.as_str()), Some("H"));
        assert_eq!(by_number(118).map(|e| e.name.as_str()), Some("Oganesson"));
        assert!(by_number(0).is_none() && by_number(119).is_none());
    }

    #[test]
    fn each_rarity_has_its_share_of_elements_and_the_weights_make_a_whole() {
        let count = |r| all().iter().filter(|e| e.rarity == r).count();
        assert_eq!(
            [
                Rarity::Common,
                Rarity::Uncommon,
                Rarity::Rare,
                Rarity::Legendary
            ]
            .map(count),
            [20, 50, 22, 26]
        );
        let total: u32 = [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Legendary,
        ]
        .iter()
        .map(|r| r.weight())
        .sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn each_description_is_one_sentence() {
        for e in all() {
            let d = &e.description;
            assert!(
                d.ends_with('.') && d.matches(". ").count() == 0,
                "{}: {d}",
                e.name
            );
            assert!(d.len() < 160, "{}: {d}", e.name);
        }
    }

    #[test]
    fn every_element_has_a_dozen_adjectives_of_its_letter_but_xenon() {
        for e in all() {
            let own = own_adjectives(e);
            let least = if e.name.starts_with('X') { 3 } else { 12 };
            assert!(own.len() >= least, "{} has {}", e.name, own.len());
            assert!(
                own.iter().all(|a| a.starts_with(&e.name[..1])),
                "{}",
                e.name
            );
        }
        assert!(general_adjectives().len() >= 150);
        for a in general_adjectives()
            .iter()
            .chain(ADJECTIVES.by_letter.values().flatten())
        {
            assert!(
                !a.contains(' ') && a.chars().next().is_some_and(char::is_uppercase),
                "{a}"
            );
        }
    }

    #[test]
    fn a_session_always_gets_the_same_name_of_an_adjective_and_its_element() {
        let (number, name) = name_of("claude-code", "c942bfdf");
        assert_eq!(name_of("claude-code", "c942bfdf"), (number, name.clone()));
        let element = by_number(number).unwrap();
        let adjective = name.strip_suffix(&format!(" {}", element.name)).unwrap();
        assert!(
            own_adjectives(element)
                .iter()
                .chain(general_adjectives())
                .any(|a| a == adjective),
            "{name}"
        );
        assert_ne!(name_of("codex", "c942bfdf").1, name, "the agent counts");
    }

    #[test]
    fn rarities_come_up_by_their_weights() {
        let ids = 20_000;
        let mut by_rarity: HashMap<Rarity, usize> = HashMap::new();
        let mut names = HashSet::new();
        for i in 0..ids {
            let (number, name) = name_of("claude-code", &format!("session-{i}"));
            *by_rarity
                .entry(by_number(number).unwrap().rarity)
                .or_default() += 1;
            names.insert(name);
        }
        let share = |r| by_rarity.get(&r).copied().unwrap_or(0) as f64 / ids as f64 * 100.0;
        assert!((share(Rarity::Common) - 60.0).abs() < 2.0, "{by_rarity:?}");
        assert!(
            (share(Rarity::Uncommon) - 30.0).abs() < 2.0,
            "{by_rarity:?}"
        );
        assert!((share(Rarity::Rare) - 9.0).abs() < 1.0, "{by_rarity:?}");
        assert!(
            (share(Rarity::Legendary) - 1.0).abs() < 0.5,
            "{by_rarity:?}"
        );
        // the adjectives spread the sessions over thousands of names
        assert!(names.len() > 8_000, "{}", names.len());
    }

    #[test]
    fn the_first_and_last_shares_land_on_the_first_and_last_elements() {
        assert_eq!(pick_element(0.0).number, 1);
        assert_eq!(pick_element(0.999_999_9).rarity, Rarity::Legendary);
    }
}
