//! Session names in SQL: the element and the generated name of a session,
//! derived from its agent and id, as functions every connection has, so
//! that queries can filter and search by them.

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

/// Adds `session_element(agent, id)` and `session_generated_name(agent, id)`.
pub(super) fn register(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("session_element", 2, flags, |ctx| {
        Ok(crate::elements::name_of(&ctx.get::<String>(0)?, &ctx.get::<String>(1)?).0)
    })?;
    conn.create_scalar_function("session_generated_name", 2, flags, |ctx| {
        Ok(crate::elements::name_of(&ctx.get::<String>(0)?, &ctx.get::<String>(1)?).1)
    })
}
