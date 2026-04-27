//! Wikitext to HTML rendering.
//!
//! Two paths, decided per article at render time:
//!
//! 1. **Cached Parsoid HTML** (preferred): if the API client has cached
//!    rendered HTML for this article+revision, serve that directly. Faithful
//!    to Wikipedia, including infoboxes, citations, and Lua-rendered content.
//! 2. **Local Rust render** (fallback): walk the `parse_wiki_text_2` AST and
//!    emit HTML for the structural elements (paragraphs, headings, lists,
//!    tables, links). Templates render as styled placeholders, except for a
//!    small library of hand-implemented common templates loaded from a TOML
//!    registry (infobox, citation, lang, math display).
//!
//! Internal links are resolved against the storage layer's offset index;
//! present articles get live links, missing ones get a visual marker.
//!
//! Implementation ships in step 4 of the build order.
