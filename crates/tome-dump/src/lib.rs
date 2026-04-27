//! Dump access layer.
//!
//! Reads the multistream bz2 dump and its companion index without ever
//! decompressing the whole file. Three responsibilities:
//!
//! - [`index`]: stream-decode the index file and emit
//!   `(stream_offset, page_id, title)` rows.
//! - [`stream`]: given a byte offset (and optional length), decompress that
//!   single bz2 stream and parse `<page>` records out of it.
//! - [`integrity`]: SHA1 verification of a dump file against the official
//!   checksum.

pub mod fixture;
pub mod index;
pub mod integrity;
pub mod stream;

pub use index::{IndexEntry, IndexReader, parse_index_line};
pub use integrity::verify_sha1;
pub use stream::{DumpReader, RawPage, parse_pages};
