//! Make Cargo rebuild this crate when the migrations change.
//!
//! `sqlx::migrate!` in `src/main.rs` embeds every file in `migrations/` into the
//! binary at compile time. Cargo does not track that directory on its own, so
//! **adding** a migration leaves the crate looking fresh and the binary keeps
//! the previous set embedded. The app then refuses to start against a database
//! that already has the new migration:
//!
//! ```text
//! Error: migration 5 was previously applied but is missing in the resolved migrations
//! ```
//!
//! Editing an *existing* migration does invalidate the build (its mtime moves),
//! which is exactly why this is easy to misdiagnose: the bug only appears when a
//! file is added, removed, or renamed.
//!
//! Watching the directory makes Cargo scan it for changes, so any addition,
//! removal or rename counts as a change to this crate.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
