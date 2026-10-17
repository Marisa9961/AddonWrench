//! Local addon management, metadata, and persistence, independent of the UI runtime.
mod core;

pub use core::{addons, browser, cache, discovery, games, groups, keyvalues, metadata, runtime};
