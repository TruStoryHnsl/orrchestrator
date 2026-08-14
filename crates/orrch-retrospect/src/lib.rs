pub mod analyzer;
pub mod error_parser;
pub mod fingerprint;
pub mod protocol;
pub mod store;
pub mod tracker;

pub use analyzer::{EcosystemAnalysis, ProjectAnalysis, analyze_ecosystem};
pub use error_parser::{ErrorCategory, classify_error, extract_errors};
pub use fingerprint::fingerprint;
pub use protocol::generate_protocols;
pub use store::{ErrorRecord, ErrorStore};
pub use tracker::SolutionTracker;
