//! Session names: each session is named after a chemical element when the
//! core first records it, such as Happy Hydrogen, and keeps the name until
//! the person renames it.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};

/// Names the sessions that have no name yet, oldest first: a new one the
/// moment its row is made, and on the first start after the migration that
/// added names, every session recorded before it. `random` gives numbers in
/// [0, 1).
pub(super) fn name_new_sessions(
    conn: &Connection,
    random: &mut dyn FnMut() -> f64,
) -> rusqlite::Result<()> {
    let unnamed: Vec<(String, String, bool)> = conn
        .prepare(
            "SELECT agent, id, archived_at IS NOT NULL FROM sessions
             WHERE element IS NULL ORDER BY first_at, agent, id",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    if unnamed.is_empty() {
        return Ok(());
    }
    // how many sessions had each element, archived ones included, and the
    // names the active sessions hold
    let mut used: HashMap<u32, usize> = conn
        .prepare(
            "SELECT element, COUNT(*) FROM sessions WHERE element IS NOT NULL GROUP BY element",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?
        .collect::<Result<_, _>>()?;
    let mut taken: HashSet<String> = conn
        .prepare("SELECT name FROM sessions WHERE name IS NOT NULL AND archived_at IS NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for (agent, id, archived) in unnamed {
        let (element, name) = crate::elements::draw(&used, &taken, random);
        conn.execute(
            "UPDATE sessions SET element = ?3, name = ?4, generated_name = ?4
             WHERE agent = ?1 AND id = ?2 AND element IS NULL",
            params![agent, id, element, name],
        )?;
        *used.entry(element).or_default() += 1;
        if !archived {
            taken.insert(name);
        }
    }
    Ok(())
}
