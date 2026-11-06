//! An item's key, such as `PIN-42`: its repository's prefix and the number
//! the repository gave it. A prefix is two to five capital letters, derived
//! from the repository's name when its first item is added.

use std::fmt;
use std::str::FromStr;

/// The fewest and the most letters a prefix has.
pub const PREFIX_MIN: usize = 2;
pub const PREFIX_MAX: usize = 5;
/// The largest number a key can carry.
pub const NUMBER_MAX: i64 = 999_999_999;

/// A key: a repository's prefix and an item's number in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub prefix: String,
    pub number: i64,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.prefix, self.number)
    }
}

impl FromStr for Key {
    type Err = String;

    /// `PIN-42`: a prefix in capitals, a hyphen, and a number from 1 without
    /// a leading zero.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (prefix, number) = text
            .split_once('-')
            .ok_or_else(|| format!("`{text}` is not a key such as PIN-42"))?;
        if !valid_prefix(prefix) {
            return Err(format!(
                "`{text}` is not a key: a key starts with {PREFIX_MIN} to {PREFIX_MAX} capital letters, such as PIN-42"
            ));
        }
        Ok(Key {
            prefix: prefix.to_string(),
            number: parse_number(number).ok_or_else(|| {
                format!("`{text}` is not a key: its number must be from 1 to {NUMBER_MAX}")
            })?,
        })
    }
}

/// An item's number, as written alone or in a key: digits, from 1, without
/// a leading zero.
pub fn parse_number(text: &str) -> Option<i64> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<i64>().ok().filter(|n| *n <= NUMBER_MAX)
}

/// Whether `prefix` is two to five capital letters A to Z.
pub fn valid_prefix(prefix: &str) -> bool {
    (PREFIX_MIN..=PREFIX_MAX).contains(&prefix.len())
        && prefix.bytes().all(|b| b.is_ascii_uppercase())
}

/// The prefix for a repository named `name`, the first of the candidates
/// that `taken` does not refuse. From the name's last part, after any `/`:
/// the first letters of its words, such as `PP` for `pinrail-plugins`, or
/// the first three letters of a single word, such as `PIN` for `pinrail`. A
/// clash takes one more letter, such as `PINR`, up to five.
pub fn derive_prefix(name: &str, taken: impl Fn(&str) -> bool) -> String {
    candidates(name)
        .into_iter()
        .find(|c| !taken(c))
        .expect("the candidates end with hundreds of letters after a stem of the name")
}

/// The prefixes to try for `name`, in order.
fn candidates(name: &str) -> Vec<String> {
    let last = name.rsplit('/').next().unwrap_or(name);
    // the words, as runs of ASCII letters; digits and other characters
    // separate them
    let words: Vec<String> = last
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_uppercase())
        .collect();
    // the letters to take from: the words' first letters, then the rest of
    // the last word, for several words; the word's letters, for one
    let (stream, base) = match words.as_slice() {
        [] => (String::new(), 0),
        [word] => (word.clone(), 3),
        many => {
            let initials: String = many.iter().map(|w| &w[..1]).collect();
            let rest = &many[many.len() - 1][1..];
            let base = initials.len().min(PREFIX_MAX);
            (format!("{initials}{rest}"), base)
        }
    };
    let mut out = Vec::new();
    let mut push = |candidate: String| {
        if valid_prefix(&candidate) && !out.contains(&candidate) {
            out.push(candidate);
        }
    };
    for len in base.max(PREFIX_MIN)..=PREFIX_MAX {
        if stream.len() >= len {
            push(stream[..len].to_string());
        }
    }
    // a name with too few letters, or every candidate taken: a stem of the
    // name's letters, padded with X, then each letter after it
    let mut stem: String = stream.chars().take(PREFIX_MAX - 1).collect();
    while stem.len() < PREFIX_MIN {
        stem.push('X');
    }
    push(stem.clone());
    let stem: String = stem.chars().take(PREFIX_MAX - 1).collect();
    for letter in 'A'..='Z' {
        push(format!("{stem}{letter}"));
    }
    // and two letters after a shorter stem, 676 more
    let short: String = stem.chars().take(PREFIX_MAX - 2).collect();
    for first in 'A'..='Z' {
        for second in 'A'..='Z' {
            push(format!("{short}{first}{second}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free(name: &str) -> String {
        derive_prefix(name, |_| false)
    }

    #[test]
    fn a_key_is_written_and_read_as_its_prefix_and_number() {
        let key: Key = "PIN-42".parse().unwrap();
        assert_eq!(
            key,
            Key {
                prefix: "PIN".into(),
                number: 42
            }
        );
        assert_eq!(key.to_string(), "PIN-42");
        assert_eq!("PP-1".parse::<Key>().unwrap().prefix, "PP");
    }

    #[test]
    fn a_key_that_is_not_well_formed_is_refused() {
        for text in [
            "pin-42",
            "Pin-42",
            "PIN 42",
            "PIN42",
            "PIN-",
            "PIN-0",
            "PIN-042",
            "PIN--4",
            "PIN-4a",
            "P-42",
            "PINRAI-42",
            "PIN-1000000000",
            "-42",
            "",
        ] {
            assert!(text.parse::<Key>().is_err(), "{text}");
        }
    }

    #[test]
    fn a_number_alone_is_digits_from_one() {
        assert_eq!(parse_number("42"), Some(42));
        assert_eq!(parse_number("#42"), Some(42));
        for text in ["0", "042", "-1", "4.2", "", "#", "1000000000"] {
            assert_eq!(parse_number(text), None, "{text}");
        }
    }

    #[test]
    fn a_prefix_is_two_to_five_capital_letters() {
        for ok in ["PP", "PIN", "PINRA"] {
            assert!(valid_prefix(ok), "{ok}");
        }
        for bad in ["P", "PINRAI", "Pin", "P1N", "PÍN", ""] {
            assert!(!valid_prefix(bad), "{bad}");
        }
    }

    #[test]
    fn a_prefix_is_derived_from_the_repositorys_name() {
        assert_eq!(free("pinrail"), "PIN");
        assert_eq!(free("forgeplane/pinrail"), "PIN");
        assert_eq!(free("pinrail-plugins"), "PP");
        assert_eq!(free("forgeplane/pinrail-plugin-experiments"), "PPE");
        assert_eq!(free("acme/api"), "API");
        assert_eq!(free("parts_storage"), "PS");
        assert_eq!(free("my.web.app"), "MWA");
        assert_eq!(free("web3"), "WEB");
        assert_eq!(free("a-b-c-d-e-f-g"), "ABCDE");
        // too few letters: padded
        assert_eq!(free("x"), "XX");
        assert_eq!(free("go"), "GO");
        assert_eq!(free("2026"), "XX");
        assert_eq!(free("café"), "CAF");
    }

    #[test]
    fn a_clash_takes_one_more_letter() {
        let taken = ["PIN", "PINR"];
        assert_eq!(derive_prefix("pinrail", |p| taken.contains(&p)), "PINRA");
        assert_eq!(derive_prefix("pinrail-plugins", |p| p == "PP"), "PPL");
        // every candidate from the name taken: a stem and each letter after it
        let taken = ["PIN", "PINR", "PINRA"];
        assert_eq!(derive_prefix("pinrail", |p| taken.contains(&p)), "PINRB");
        assert_eq!(derive_prefix("go", |p| p == "GO"), "GOA");
    }

    #[test]
    fn every_prefix_derived_is_valid_and_unique_among_the_taken() {
        let mut taken: Vec<String> = Vec::new();
        for _ in 0..100 {
            let next = derive_prefix("pinrail", |p| taken.iter().any(|t| t == p));
            assert!(valid_prefix(&next), "{next}");
            assert!(!taken.contains(&next));
            taken.push(next);
        }
    }
}
