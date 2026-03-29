pub mod analyzer;
pub mod error_parser;
pub mod fingerprint;
pub mod protocol;
pub mod store;
pub mod tracker;

pub use analyzer::{analyze_ecosystem, EcosystemAnalysis, ProjectAnalysis};
pub use error_parser::{classify_error, extract_errors, ErrorCategory};
pub use fingerprint::fingerprint;
pub use protocol::generate_protocols;
pub use store::{ErrorRecord, ErrorStore};
pub use tracker::SolutionTracker;
