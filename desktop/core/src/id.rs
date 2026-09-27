//! Review ids: `r_` plus a ULID, so lexicographic order is creation order.
//!
//! The entropy half is a per-process random salt followed by a counter, so
//! ids minted in the same millisecond still sort in creation order, and no
//! id sorts below the last one issued, even when the clock steps back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const PREFIX: &str = "r_";

static COUNTER: AtomicU64 = AtomicU64::new(0);
static SALT: OnceLock<u32> = OnceLock::new();
/// The last id issued, as a number: no later one may sort below it.
static LAST: Mutex<u128> = Mutex::new(0);

/// The next review id: greater than any this process returned before.
pub fn next() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    next_at(ms)
}

/// The next id for a clock that reads `ms`.
fn next_at(ms: u64) -> String {
    let ms = ms & ((1 << 48) - 1);
    let salt = *SALT.get_or_init(seed);
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst) & ((1 << 48) - 1);
    let candidate = ((ms as u128) << 80) | ((salt as u128) << 48) | counter as u128;
    // a clock that steps back (NTP, a manual change) must not reorder ids
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let bits = candidate.max(*last + 1);
    *last = bits;
    format!("{PREFIX}{}", encode(bits))
}

/// True if the string is shaped like a review id.
pub fn is_valid(s: &str) -> bool {
    let Some(rest) = s.strip_prefix(PREFIX) else {
        return false;
    };
    rest.len() == 26 && rest.as_bytes()[0] <= b'7' && rest.bytes().all(|b| ALPHABET.contains(&b))
}

fn seed() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = nanos ^ ((std::process::id() as u64) << 32) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    x as u32
}

fn encode(bits: u128) -> String {
    (0..26)
        .map(|i| ALPHABET[((bits >> (125 - 5 * i)) & 31) as usize] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_clock_that_steps_back_does_not_reorder_ids() {
        // listing, paging and rounds sort by id: an NTP step backwards must
        // not put a new review below older ones
        let before = super::next_at(2_000_000);
        let after = super::next_at(1_000_000);
        assert!(after > before, "{after} sorts below {before}");
    }

    use super::*;

    #[test]
    fn ids_are_valid_and_ordered() {
        let a = next();
        let b = next();
        assert!(is_valid(&a), "{a}");
        assert!(is_valid(&b), "{b}");
        assert!(a < b);
        assert_eq!(a.len(), 28);
    }

    #[test]
    fn validity_is_strict() {
        assert!(!is_valid("g_01ARZ3NDEKTSV4RRFFQ69G5FAV"));
        assert!(!is_valid("r_short"));
        assert!(!is_valid("r_8ZZZZZZZZZZZZZZZZZZZZZZZZZ"));
        assert!(is_valid("r_01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    }
}
