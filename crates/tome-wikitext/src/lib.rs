//! Wikitext to HTML rendering.
//!
//! Two paths, decided per article at access time:
//!
//! 1. **Cached Parsoid HTML** (preferred): when the API client has already
//!    cached rendered HTML for this article+revision, that is served directly.
//!    Faithful to Wikipedia, including infoboxes, citations, and Lua-rendered
//!    content. That path lives in `tome-api` and is composed by
//!    `tome-services` — this crate is not involved.
//!
//! 2. **Local Rust render** (offline fallback): walk the
//!    [`parse_wiki_text_2`] AST and emit HTML for the structural elements
//!    (paragraphs, headings, lists, internal/external links, plain text).
//!    Templates render as styled placeholders; references are collected into
//!    a footnote list. Output is intentionally lower fidelity but readable.
//!
//! Internal links are resolved against an injected [`LinkResolver`] so the
//! renderer is decoupled from storage and reusable in tests with mock
//! resolvers.

pub mod escape;
pub mod link;
pub mod render;

pub use link::{LinkResolver, LinkStatus, NoopLinkResolver};
pub use render::{RenderOptions, Renderer};
