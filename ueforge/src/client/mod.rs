//! Blocking test client.
//!
//! Moved into `modforge::client` during Phase 0b row 17.
//! Re-exported here so existing ueforge call sites compile unchanged.

pub use modforge::client::*;
pub use modforge::client::{diff, perf, scenario};
