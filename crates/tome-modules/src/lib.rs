//! Module manager.
//!
//! A module is a named collection of articles defined by Wikipedia categories
//! (with configurable depth) and/or arbitrary article lists. This crate owns:
//!
//! - Category tree resolution via the API client
//! - Module install / uninstall (delegating tier moves to storage)
//! - Default tier and disk-usage accounting per module
//! - Import/export of modules as portable files
//! - Loading the bundled "Suggested Modules" library from a TOML config
//!
//! Implementation ships in step 7 of the build order.
