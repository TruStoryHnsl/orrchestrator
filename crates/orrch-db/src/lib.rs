//! `orrch-db`: an ephemeral, rebuildable SQLite query layer for orrchestrator.
//!
//! Source of truth is markdown in each project's `.orrch/` folder plus the
//! global library. The database holds zero unique durable state and is fully
//! reconstructable from those files. There are no schema migrations.

pub mod model;
pub mod schema;
pub mod parse;
pub mod ingest;
pub mod fold;
pub mod rebuild;
pub mod query;
pub mod watch;
pub mod migrate;

pub use model::{
    ArchFact, BugRow, EntityType, EventKind, EventRecord, FeatureRow, LibraryRow, LicenseRow,
};
// pub use rebuild::{...}; // Task 7
