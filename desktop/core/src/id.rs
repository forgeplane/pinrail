//! Review ids: `r_` plus a ULID, so lexicographic order is creation order.
//!
//! The entropy half is a per-process random salt followed by a counter, so
//! ids minted in the same millisecond still sort in creation order.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const PREFIX: &str = "r_";

static COUNTER: AtomicU64 = AtomicU64::new(0);
static SALT: OnceLock<u32> = OnceLock::new();

/// The next review id: greater than any this process returned before.
pub fn next() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        & ((1 << 48) - 1);
    let salt = *SALT.get_or_init(seed);
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst) & ((1 << 48) - 1);
    let bits = ((ms as u128) << 80) | ((salt as u128) << 48) | counter as u128;
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
