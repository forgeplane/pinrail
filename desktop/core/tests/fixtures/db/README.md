# Databases from earlier releases

`pinrail-0.6.0.db` was written by Pinrail 0.6.0 through its own API: a
session with statuses, a note, a message for its agent, a question still
waiting and a review decided and starred; an archived session; a project
with its repository; two work items, one a sub-item with a comment, claimed by the
person and in progress. `tests/upgrade.rs` opens a copy with this build, which runs every
migration since.
