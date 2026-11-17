//! The chemical elements a session is named after, with the adjectives of
//! the names: the 118 elements with their place on the periodic table,
//! their category, how rare a session's draw makes them and one sentence
//! about each, and the adjectives by first letter, with a general list
//! for when an element's own are taken.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
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

/// The element and the name of a new session. The element is drawn at
/// random, by its rarity's weight, an element no session has had (none in
/// `used`) twice as likely as one of its rarity that some have. The
/// adjective comes from the element's own, then from the general list,
/// skipping any name in `taken`, the names of the active sessions.
/// `random` gives numbers in [0, 1).
pub fn draw(
    used: &HashMap<u32, usize>,
    taken: &HashSet<String>,
    random: &mut dyn FnMut() -> f64,
) -> (u32, String) {
    // a few elements, in case each name of one is taken
    let mut fallback = None;
    for _ in 0..8 {
        let element = pick_element(used, random);
        let own = own_adjectives(element);
        let general = general_adjectives();
        let own_from = start_of(own.len(), random());
        let general_from = start_of(general.len(), random());
        let mut names = rotated(own, own_from)
            .chain(rotated(general, general_from))
            .map(|adjective| format!("{adjective} {}", element.name));
        let first = names.next();
        if let Some(name) = first
            .iter()
            .cloned()
            .chain(names)
            .find(|n| !taken.contains(n))
        {
            return (element.number, name);
        }
        fallback.get_or_insert((
            element.number,
            first.unwrap_or_else(|| element.name.clone()),
        ));
    }
    fallback.expect("eight draws were made")
}

/// Where to start in a list of `len`, for a random number in [0, 1).
fn start_of(len: usize, random: f64) -> usize {
    ((random * len as f64) as usize).min(len.saturating_sub(1))
}

/// The list from `from`, then round to the item before it.
fn rotated(list: &'static [String], from: usize) -> impl Iterator<Item = &'static String> {
    list.iter().cycle().skip(from).take(list.len())
}

/// An element drawn by weight: its rarity's share of the draws, split
/// among the rarity's elements, an unused one counting twice.
fn pick_element(used: &HashMap<u32, usize>, random: &mut dyn FnMut() -> f64) -> &'static Element {
    let fresh = |e: &Element| {
        if used.get(&e.number).copied().unwrap_or(0) == 0 {
            2.0
        } else {
            1.0
        }
    };
    let share =
        |rarity: Rarity| -> f64 { all().iter().filter(|e| e.rarity == rarity).map(fresh).sum() };
    let shares: HashMap<Rarity, f64> = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Legendary,
    ]
    .into_iter()
    .map(|r| (r, share(r)))
    .collect();
    let weight = |e: &Element| f64::from(e.rarity.weight()) * fresh(e) / shares[&e.rarity];
    let total: f64 = all().iter().map(weight).sum();
    let mut at = random() * total;
    for e in all() {
        at -= weight(e);
        if at < 0.0 {
            return e;
        }
    }
    all().last().expect("the list has elements")
}

/// Random numbers in [0, 1), from the operating system.
pub fn system_random() -> f64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the operating system's random numbers");
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A random source that cycles through the given numbers.
    fn seq(numbers: &[f64]) -> impl FnMut() -> f64 + '_ {
        let mut i = 0;
        move || {
            i += 1;
            numbers[(i - 1) % numbers.len()]
        }
    }

    #[test]
    fn a_name_is_an_alliterative_adjective_and_its_element() {
        let (number, name) = draw(&HashMap::new(), &HashSet::new(), &mut seq(&[0.0]));
        // the first element of the first rarity, the first of its adjectives
        assert_eq!(number, 1);
        let adjective = name.strip_suffix(" Hydrogen").expect("the element's name");
        assert!(
            own_adjectives(by_number(1).unwrap())
                .iter()
                .any(|a| a == adjective)
        );
    }

    #[test]
    fn a_name_an_active_session_holds_is_skipped_then_the_general_list_follows() {
        let hydrogen = by_number(1).unwrap();
        let mut taken: HashSet<String> = own_adjectives(hydrogen)
            .iter()
            .map(|a| format!("{a} Hydrogen"))
            .collect();
        let (_, name) = draw(&HashMap::new(), &taken, &mut seq(&[0.0]));
        let adjective = name.strip_suffix(" Hydrogen").unwrap();
        assert!(
            general_adjectives().iter().any(|a| a == adjective),
            "{name}"
        );
        // every name of Hydrogen taken: another element is drawn
        taken.extend(general_adjectives().iter().map(|a| format!("{a} Hydrogen")));
        // (the first draw lands on Hydrogen again, the second elsewhere)
        let (number, _) = draw(
            &HashMap::new(),
            &taken,
            &mut seq(&[0.0, 0.0, 0.0, 0.5, 0.0, 0.0]),
        );
        assert_ne!(number, 1);
    }

    #[test]
    fn rarities_come_up_by_their_weights_and_unused_elements_twice_as_often() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut random = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let draws = 20_000;
        let mut by_rarity: HashMap<Rarity, usize> = HashMap::new();
        for _ in 0..draws {
            *by_rarity
                .entry(pick_element(&HashMap::new(), &mut random).rarity)
                .or_default() += 1;
        }
        let share = |r| by_rarity.get(&r).copied().unwrap_or(0) as f64 / draws as f64 * 100.0;
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

        // with every common element used but Hydrogen, Hydrogen is drawn
        // about twice as often as each of the others
        let used: HashMap<u32, usize> = all()
            .iter()
            .filter(|e| e.rarity == Rarity::Common && e.number != 1)
            .map(|e| (e.number, 1))
            .collect();
        let mut hits: HashMap<u32, usize> = HashMap::new();
        for _ in 0..draws {
            *hits
                .entry(pick_element(&used, &mut random).number)
                .or_default() += 1;
        }
        let carbon = hits[&6] as f64;
        let ratio = hits[&1] as f64 / carbon;
        assert!(
            (1.6..2.5).contains(&ratio),
            "Hydrogen {} vs Carbon {carbon}",
            hits[&1]
        );
    }
}
