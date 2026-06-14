//! Streaming parser for Wikipedia's `geo_tags.sql.gz` dump file.
//!
//! The file is a gzipped MySQL dump containing many `INSERT INTO geo_tags
//! VALUES (...), (...), ...;` statements. Schema columns (in order):
//!
//! ```text
//! gt_id, gt_page_id, gt_globe, gt_primary, gt_lat, gt_lon,
//! gt_dim, gt_type, gt_name, gt_country, gt_region
//! ```
//!
//! We extract `gt_page_id`, `gt_primary`, `gt_lat`, `gt_lon`, and `gt_type`
//! (as `kind`). Other columns are discarded.
//!
//! The parser is purpose-built for this format — not a general-purpose SQL
//! parser. It correctly handles single-quoted strings with `\'` and `\\`
//! escape sequences, `NULL` literals, integers, and floats.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use flate2::read::GzDecoder;
use tome_core::{Result, TomeError};
use tome_storage::Geotag;

const INSERT_PREFIX: &str = "INSERT INTO `geo_tags` VALUES ";

/// Decompressed bytes pulled from the gzip stream per read.
const CHUNK_SIZE: usize = 1 << 20; // 1 MiB

/// `cancel` is checked once per decompressed chunk (~1 MiB), so a user's
/// cancel click takes effect within milliseconds even on a multi-GB dump;
/// returns [`TomeError::Cancelled`] so the caller can distinguish "user
/// changed their mind" from a real parse failure.
pub fn parse_file<F: FnMut(Geotag)>(
    path: &Path,
    cancel: &AtomicBool,
    mut on_geotag: F,
) -> Result<u64> {
    let file = File::open(path)
        .map_err(|e| TomeError::Other(format!("open geotag dump {path:?}: {e}")))?;
    // Stream the gzip in bounded chunks instead of decompressing the whole
    // dump into one String. `pending` holds only the unparsed tail — at most
    // one in-flight INSERT statement — so resident memory is independent of
    // dump size. An INSERT statement or tuple that straddles a chunk boundary
    // stays in `pending` and is completed once the next chunk is read, so we
    // don't rely on the dump being newline-framed.
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
            .map_err(|e| TomeError::Other(format!("decompress geotag dump: {e}")))?;
        if n == 0 {
            // Clean EOF: flush the remainder, allowing a final statement that
            // isn't terminated by a trailing newline.
            count += parse_buf(&pending, true, &mut on_geotag).0;
            break;
        }
        pending.extend_from_slice(&chunk[..n]);
        let (parsed, consumed) = parse_buf(&pending, false, &mut on_geotag);
        count += parsed;
        pending.drain(..consumed);
    }
    Ok(count)
}

/// Parse a complete, in-memory SQL string. Retained for the unit tests that
/// feed small `&str` fixtures; `parse_file` drives the same core incrementally
/// via [`parse_buf`].
pub fn parse_str<F: FnMut(Geotag)>(content: &str, mut on_geotag: F) -> Result<u64> {
    Ok(parse_buf(content.as_bytes(), true, &mut on_geotag).0)
}

/// Parse every complete `INSERT INTO ... VALUES (...);` statement present in
/// `buf`, invoking `on_geotag` for each row.
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
fn parse_buf<F: FnMut(Geotag)>(buf: &[u8], eof: bool, on_geotag: &mut F) -> (u64, usize) {
    let prefix = INSERT_PREFIX.as_bytes();
    let mut count = 0_u64;
    let mut pos = 0;
    loop {
        let Some(rel) = find_subslice(&buf[pos..], prefix) else {
            // No further prefix. Retain a possible partial prefix at the tail
            // so a prefix split across the boundary still matches next time.
            let keep = if eof {
                0
            } else {
                prefix_tail_overlap(buf, prefix)
            };
            return (count, buf.len() - keep);
        };
        let stmt_start = pos + rel;
        let mut i = stmt_start + prefix.len();
        // Buffer this statement's rows; only emit once we reach its `;` (or
        // EOF). A statement straddling the boundary is then retained intact
        // rather than half-emitted and re-emitted on the next chunk.
        let mut rows: Vec<Geotag> = Vec::new();
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
            if let Some(g) = fields_to_geotag(&fields) {
                rows.push(g);
            }
        };
        for g in rows {
            on_geotag(g);
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
            b',' => {
                fields.push(take_field(&mut current));
            }
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

fn fields_to_geotag(fields: &[String]) -> Option<Geotag> {
    if fields.len() < 6 {
        return None;
    }
    let page_id: u64 = fields[1].parse().ok()?;
    let primary = fields[3] == "1";
    let lat: f64 = fields[4].parse().ok()?;
    let lon: f64 = fields[5].parse().ok()?;
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let kind = if fields.len() > 7 && fields[7] != "NULL" && !fields[7].is_empty() {
        Some(fields[7].clone())
    } else {
        None
    };
    Some(Geotag {
        page_id,
        lat,
        lon,
        primary,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_insert_with_two_rows() {
        let sql = "\
INSERT INTO `geo_tags` VALUES \
(1,23535,'earth',1,42.5,-71.0,1000,'mountain','Foo','US',NULL),\
(2,9404,'earth',0,51.5,-0.1,500,'city',NULL,'GB',NULL);";
        let mut tags = Vec::new();
        let n = parse_str(sql, |g| tags.push(g)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(tags[0].page_id, 23535);
        assert!(tags[0].primary);
        assert!((tags[0].lat - 42.5).abs() < 1e-6);
        assert_eq!(tags[0].kind.as_deref(), Some("mountain"));
        assert_eq!(tags[1].page_id, 9404);
        assert!(!tags[1].primary);
        assert_eq!(tags[1].kind.as_deref(), Some("city"));
    }

    #[test]
    fn handles_quoted_strings_with_escaped_apostrophe() {
        // gt_name = "Joan d'Arc memorial"
        let sql = "INSERT INTO `geo_tags` VALUES (1,1,'earth',1,1.0,2.0,0,'landmark','Joan d\\'Arc memorial','FR',NULL);";
        let mut tags = Vec::new();
        let n = parse_str(sql, |g| tags.push(g)).unwrap();
        assert_eq!(n, 1);
        assert_eq!(tags[0].kind.as_deref(), Some("landmark"));
    }

    #[test]
    fn skips_invalid_coordinates() {
        let sql =
            "INSERT INTO `geo_tags` VALUES (1,1,'earth',1,999.0,500.0,0,'broken',NULL,NULL,NULL);";
        let mut tags = Vec::new();
        let n = parse_str(sql, |g| tags.push(g)).unwrap();
        assert_eq!(n, 0);
        assert!(tags.is_empty());
    }

    #[test]
    fn null_kind_is_none() {
        let sql = "INSERT INTO `geo_tags` VALUES (1,42,'earth',1,10.0,20.0,0,NULL,NULL,NULL,NULL);";
        let mut tags = Vec::new();
        parse_str(sql, |g| tags.push(g)).unwrap();
        assert_eq!(tags[0].kind, None);
    }

    #[test]
    fn ignores_non_geotag_inserts() {
        let sql = "INSERT INTO `something_else` VALUES (1,2,3); INSERT INTO `geo_tags` VALUES (1,42,'earth',1,10.0,20.0,0,'city',NULL,NULL,NULL);";
        let mut tags = Vec::new();
        parse_str(sql, |g| tags.push(g)).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].page_id, 42);
    }

    #[test]
    fn multiple_insert_blocks_in_one_file() {
        let sql = "\
INSERT INTO `geo_tags` VALUES (1,1,'earth',1,1.0,1.0,0,'a',NULL,NULL,NULL);\n\
INSERT INTO `geo_tags` VALUES (2,2,'earth',1,2.0,2.0,0,'b',NULL,NULL,NULL);\n\
INSERT INTO `geo_tags` VALUES (3,3,'earth',1,3.0,3.0,0,'c',NULL,NULL,NULL);";
        let mut tags = Vec::new();
        let n = parse_str(sql, |g| tags.push(g)).unwrap();
        assert_eq!(n, 3);
        assert_eq!(tags[2].page_id, 3);
    }

    #[test]
    fn handles_statements_split_across_chunk_boundaries() {
        // Feed the SQL one byte at a time through `parse_buf`, exactly as
        // `parse_file` drives it, so every prefix, tuple, and `;` straddles a
        // "chunk" boundary. The retain-and-re-parse path must yield the same
        // rows as parsing the whole buffer at once, each row exactly once.
        let sql = "\
INSERT INTO `geo_tags` VALUES \
(1,11,'earth',1,42.5,-71.0,1000,'mountain','Foo','US',NULL),\
(2,22,'earth',0,51.5,-0.1,500,'city',NULL,'GB',NULL);\n\
INSERT INTO `geo_tags` VALUES (3,33,'earth',1,10.0,20.0,0,'landmark',NULL,NULL,NULL);";

        let mut streamed = Vec::new();
        {
            let mut pending: Vec<u8> = Vec::new();
            let mut push = |g: Geotag| streamed.push(g);
            for &b in sql.as_bytes() {
                pending.push(b);
                let (_, consumed) = parse_buf(&pending, false, &mut push);
                pending.drain(..consumed);
            }
            let _ = parse_buf(&pending, true, &mut push);
        }

        let mut whole = Vec::new();
        parse_str(sql, |g| whole.push(g)).unwrap();

        assert_eq!(streamed.len(), 3);
        assert_eq!(streamed.len(), whole.len());
        assert_eq!(streamed[0].page_id, 11);
        assert_eq!(streamed[2].page_id, 33);
        assert_eq!(streamed[2].kind.as_deref(), Some("landmark"));
    }
}
