//! Streaming parser for Wikipedia's `categorylinks.sql.gz` dump file.
//!
//! Schema columns (in order):
//!
//! ```text
//! cl_from, cl_to, cl_sortkey, cl_timestamp, cl_sortkey_prefix,
//! cl_collation, cl_type
//! ```
//!
//! We extract `cl_from`, `cl_to`, and `cl_type`. Sortkeys, timestamps, and
//! collation are display-side hints we don't need.
//!
//! The parser shares its tuple-extraction core with `geotag_ingest`; the
//! per-format work is just the column projection.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use flate2::read::GzDecoder;
use tome_core::{Result, TomeError};
use tome_storage::{CategoryLink, CategoryMemberKind};

const INSERT_PREFIX: &str = "INSERT INTO `categorylinks` VALUES ";

/// Decompressed bytes pulled from the gzip stream per read.
const CHUNK_SIZE: usize = 1 << 20; // 1 MiB

/// `cancel` is checked once per decompressed chunk (~1 MiB), so a user's
/// cancel click takes effect within milliseconds even on the multi-GB
/// enwiki categorylinks dump; returns [`TomeError::Cancelled`] so the
/// caller can distinguish "user changed their mind" from a parse failure.
pub fn parse_file<F: FnMut(CategoryLink)>(
    path: &Path,
    cancel: &AtomicBool,
    mut on_link: F,
) -> Result<u64> {
    let file = File::open(path)
        .map_err(|e| TomeError::Other(format!("open categorylinks dump {path:?}: {e}")))?;
    // Stream the gzip in bounded chunks instead of decompressing the whole
    // dump into one String. `pending` holds only the unparsed tail — at most
    // one in-flight INSERT statement — so resident memory is independent of
    // dump size (full enwiki categorylinks is ~2.4 GB compressed / many GB
    // raw / ~250M rows). An INSERT statement or tuple that straddles a chunk
    // boundary stays in `pending` and is completed once the next chunk is
    // read, so we don't rely on the dump being newline-framed.
    let mut decoder = GzDecoder::new(BufReader::new(file));
    let mut chunk = vec![0_u8; CHUNK_SIZE];
    let mut pending: Vec<u8> = Vec::new();
    let mut count = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(TomeError::Cancelled);
        }
        let n = decoder
            .read(&mut chunk)
            .map_err(|e| TomeError::Other(format!("decompress categorylinks dump: {e}")))?;
        if n == 0 {
            // Clean EOF: flush the remainder, allowing a final statement that
            // isn't terminated by a trailing newline.
            count += parse_buf(&pending, true, &mut on_link).0;
            break;
        }
        pending.extend_from_slice(&chunk[..n]);
        let (parsed, consumed) = parse_buf(&pending, false, &mut on_link);
        count += parsed;
        pending.drain(..consumed);
    }
    Ok(count)
}

/// Parse a complete, in-memory SQL string. Retained for the unit tests that
/// feed small `&str` fixtures; `parse_file` drives the same core incrementally
/// via [`parse_buf`].
pub fn parse_str<F: FnMut(CategoryLink)>(content: &str, mut on_link: F) -> Result<u64> {
    Ok(parse_buf(content.as_bytes(), true, &mut on_link).0)
}

/// Parse every complete `INSERT INTO ... VALUES (...);` statement present in
/// `buf`, invoking `on_link` for each row.
///
/// Returns `(rows_emitted, consumed)`, where `consumed` is the number of
/// leading bytes that are fully processed and may be discarded. Bytes from
/// `consumed` onward are an incomplete trailing unit — a partial INSERT
/// prefix, or a statement whose terminating `;` hasn't arrived — that the
/// caller must retain and re-parse once more data is read. The rows of an
/// incomplete trailing statement are buffered and only emitted once it is seen
/// whole, so nothing is double counted across chunk boundaries.
///
/// When `eof` is true no more data follows, so the whole buffer is consumed and
/// a final unterminated statement is flushed on a best-effort basis.
fn parse_buf<F: FnMut(CategoryLink)>(buf: &[u8], eof: bool, on_link: &mut F) -> (u64, usize) {
    let prefix = INSERT_PREFIX.as_bytes();
    let mut count = 0_u64;
    let mut pos = 0;
    loop {
        let Some(rel) = find_subslice(&buf[pos..], prefix) else {
            // No further prefix. Retain a possible partial prefix at the tail
            // so a prefix split across the boundary still matches next time.
            let keep = if eof { 0 } else { prefix_tail_overlap(buf, prefix) };
            return (count, buf.len() - keep);
        };
        let stmt_start = pos + rel;
        let mut i = stmt_start + prefix.len();
        // Buffer this statement's rows; only emit once we reach its `;` (or
        // EOF). A statement straddling the boundary is then retained intact
        // rather than half-emitted and re-emitted on the next chunk.
        let mut rows: Vec<CategoryLink> = Vec::new();
        let stmt_end = loop {
            while i < buf.len() && buf[i] != b'(' && buf[i] != b';' {
                i += 1;
            }
            if i >= buf.len() {
                if eof {
                    break buf.len();
                }
                return (count, stmt_start);
            }
            if buf[i] == b';' {
                break i + 1;
            }
            i += 1; // consume '('
            let Some((fields, advance)) = parse_tuple(&buf[i..]) else {
                // The tuple's closing `)` hasn't arrived yet.
                if eof {
                    break buf.len();
                }
                return (count, stmt_start);
            };
            i += advance;
            if let Some(link) = fields_to_link(&fields) {
                rows.push(link);
            }
        };
        for link in rows {
            on_link(link);
            count += 1;
        }
        pos = stmt_end;
    }
}

/// Index of the first occurrence of `needle` in `haystack` (`needle` is the
/// non-empty INSERT prefix).
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Length of the longest suffix of `buf` that is a prefix of `needle` (capped
/// at `needle.len() - 1`): the partial INSERT prefix that may complete once the
/// next chunk is appended, so it must be kept.
fn prefix_tail_overlap(buf: &[u8], needle: &[u8]) -> usize {
    let mut k = needle.len().saturating_sub(1).min(buf.len());
    while k > 0 {
        if buf[buf.len() - k..] == needle[..k] {
            return k;
        }
        k -= 1;
    }
    0
}

/// Parse one `(...)` tuple from the front of `input`. Returns the field strings
/// and the number of bytes consumed (through the closing `)`), or `None` if
/// `input` ends before the closing `)` — i.e. the tuple straddles a chunk
/// boundary and more data is needed.
fn parse_tuple(input: &[u8]) -> Option<(Vec<String>, usize)> {
    let mut fields: Vec<String> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escape = false;
    let mut i = 0;
    while i < input.len() {
        let b = input[i];
        i += 1;
        if escape {
            current.push(b);
            escape = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape = true,
                b'\'' => in_string = false,
                _ => current.push(b),
            }
            continue;
        }
        match b {
            b'\'' => in_string = true,
            b',' => fields.push(take_field(&mut current)),
            b')' => {
                fields.push(take_field(&mut current));
                return Some((fields, i));
            }
            _ => current.push(b),
        }
    }
    None
}

fn take_field(buf: &mut Vec<u8>) -> String {
    let s = String::from_utf8_lossy(buf).trim().to_string();
    buf.clear();
    s
}

fn fields_to_link(fields: &[String]) -> Option<CategoryLink> {
    if fields.len() < 7 {
        return None;
    }
    let from_page_id: u64 = fields[0].parse().ok()?;
    let category = fields[1].clone();
    if category.is_empty() || category == "NULL" {
        return None;
    }
    let kind = CategoryMemberKind::parse(&fields[6])?;
    Some(CategoryLink {
        from_page_id,
        category,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_insert_with_two_rows() {
        let sql = "\
INSERT INTO `categorylinks` VALUES \
(23535,'Photons','Photon','2024-01-01 00:00:00','','uca-default','page'),\
(9404,'Subatomic_particles','Electron','2024-01-01 00:00:00','','uca-default','page');";
        let mut links = Vec::new();
        let n = parse_str(sql, |l| links.push(l)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(links[0].from_page_id, 23535);
        assert_eq!(links[0].category, "Photons");
        assert_eq!(links[0].kind, CategoryMemberKind::Page);
        assert_eq!(links[1].kind, CategoryMemberKind::Page);
    }

    #[test]
    fn handles_subcat_and_file_kinds() {
        let sql = "\
INSERT INTO `categorylinks` VALUES \
(1,'A','','2024-01-01 00:00:00','','uca-default','subcat'),\
(2,'B','','2024-01-01 00:00:00','','uca-default','file');";
        let mut links = Vec::new();
        parse_str(sql, |l| links.push(l)).unwrap();
        assert_eq!(links[0].kind, CategoryMemberKind::Subcat);
        assert_eq!(links[1].kind, CategoryMemberKind::File);
    }

    #[test]
    fn rejects_unknown_kind() {
        let sql = "INSERT INTO `categorylinks` VALUES (1,'A','','2024-01-01 00:00:00','','uca-default','weird');";
        let mut links = Vec::new();
        let n = parse_str(sql, |l| links.push(l)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn handles_apostrophe_in_category_name() {
        let sql = "INSERT INTO `categorylinks` VALUES (1,'Joan_d\\'Arc','','2024-01-01 00:00:00','','uca-default','page');";
        let mut links = Vec::new();
        parse_str(sql, |l| links.push(l)).unwrap();
        assert_eq!(links[0].category, "Joan_d'Arc");
    }

    #[test]
    fn skips_empty_categories() {
        let sql = "INSERT INTO `categorylinks` VALUES (1,'','','2024-01-01 00:00:00','','uca-default','page');";
        let mut links = Vec::new();
        let n = parse_str(sql, |l| links.push(l)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn ignores_other_inserts() {
        let sql = "INSERT INTO `something_else` VALUES (1,2,3); INSERT INTO `categorylinks` VALUES (1,'X','','2024-01-01 00:00:00','','uca-default','page');";
        let mut links = Vec::new();
        parse_str(sql, |l| links.push(l)).unwrap();
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn handles_statements_split_across_chunk_boundaries() {
        // Feed the SQL one byte at a time through `parse_buf`, exactly as
        // `parse_file` drives it, so every prefix, tuple, and `;` straddles a
        // "chunk" boundary. The retain-and-re-parse path must yield the same
        // rows as parsing the whole buffer at once, each row exactly once.
        let sql = "\
INSERT INTO `categorylinks` VALUES \
(1,'Physics','Photon','2024-01-01 00:00:00','','uca-default','page'),\
(2,'Chemistry','Atom','2024-01-01 00:00:00','','uca-default','subcat');\n\
INSERT INTO `categorylinks` VALUES \
(3,'Biology','Cell','2024-01-01 00:00:00','','uca-default','file');";

        let mut streamed = Vec::new();
        {
            let mut pending: Vec<u8> = Vec::new();
            let mut push = |l: CategoryLink| streamed.push(l);
            for &b in sql.as_bytes() {
                pending.push(b);
                let (_, consumed) = parse_buf(&pending, false, &mut push);
                pending.drain(..consumed);
            }
            let _ = parse_buf(&pending, true, &mut push);
        }

        let mut whole = Vec::new();
        parse_str(sql, |l| whole.push(l)).unwrap();

        assert_eq!(streamed.len(), 3);
        assert_eq!(streamed.len(), whole.len());
        assert_eq!(streamed[0].from_page_id, 1);
        assert_eq!(streamed[2].category, "Biology");
        assert_eq!(streamed[2].kind, CategoryMemberKind::File);
    }
}
