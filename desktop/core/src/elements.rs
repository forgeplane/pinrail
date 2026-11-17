//! The chemical elements a session is named after, with the adjectives of
//! the names: the 118 elements with their place on the periodic table,
//! their category, how rare a session's draw makes them and one sentence
//! about each, and the adjectives by first letter, with a general list
//! for when an element's own are taken.

use serde::{Deserialize, Serialize};
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

/// The adjectives for any element, once its own are taken.
pub fn general_adjectives() -> &'static [String] {
    &ADJECTIVES.general
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
}
