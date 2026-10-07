#![forbid(unsafe_code)]

mod core;
mod error;
mod store;

pub use core::{
    AppCore, AppEvent, AppEventSink, AppEventType, ProviderContextFactory, SearchScope, SyncResult,
};
pub use error::{AppCoreError, StoreError};
pub use store::{AppStore, OutboxEntry, OutboxFailure, OutboxState, SyncCommit};

#[cfg(test)]
mod tests;
