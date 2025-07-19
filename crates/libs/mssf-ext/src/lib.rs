// This is needed because windows_core macro looks for the `windows_core` token.
extern crate mssf_pal as windows_core;

pub mod client;
pub mod traits;
pub mod types;

pub mod data;
pub mod operation;
pub mod state_provider;
pub mod state_replicator;
pub mod stream;
