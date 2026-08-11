/*! CleanOS scan core library: collect, classify, rank, report, benchmark. */

pub mod bench;
pub mod classifier;
pub mod error;
pub mod inventory;
pub mod model;
pub mod parsers;
pub mod paths;
pub mod probes;
pub mod ranker;
pub mod redaction;
pub mod reporter;

pub use error::CleanOsError;
pub use model::{RankedFinding, ReportDocument, RunSnapshot};
pub use probes::collect_run;
pub use reporter::{build_report, format_report_table, write_report};
