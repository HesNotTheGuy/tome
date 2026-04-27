//! Module manager.
//!
//! A module is a named collection of articles, defined by Wikipedia
//! categories (with depth) and/or arbitrary explicit titles. This crate is
//! the **data layer**: types, TOML import/export, and a SQLite-backed
//! [`ModuleStore`] tracking what's installed.
//!
//! The actual orchestration — resolving categories via the MediaWiki API,
//! moving members into the storage layer, fetching cached Parsoid HTML — is
//! composed in `tome-services`. The [`CategoryResolver`] trait declared here
//! lets that composition inject any resolver implementation (real API,
//! mock, fixture-driven test).

pub mod resolver;
pub mod spec;
pub mod store;

pub use resolver::{CategoryResolver, NoopResolver};
pub use spec::{CategorySpec, ModuleSpec};
pub use store::{InstalledModule, ModuleStore};
