//! Streaming parser for Wikipedia's `redirect.sql.gz` dump file.
//!
//! Schema columns (in order):
//!
//! ```text
//! rd_from, rd_namespace, rd_title, rd_interwiki, rd_fragment
//! ```
//!
//! We extract `rd_from` and `rd_title`, filtering to records where
//! `rd_namespace == 0` and `rd_interwiki` is empty so only main-namespace
//! local redirects are yielded. Underscores in titles are normalized to
//! spaces (Wikipedia URL form to display form).
//!
//! Shares its tuple-extraction core with `geotag_ingest` and
//! `category_ingest`; only the column projection and filter differ.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use flate2::read::GzDecoder;
use tome_core::{Result, TomeError};
use tome_storage::Redirect;

const INSERT_PREFIX: &str = "INSERT INTO `redirect` VALUES ";

/// Decompressed bytes pulled from the gzip stream per read.
const CHUNK_SIZE: usize = 1 << 20; // 1 MiB

/// `cancel` is checked once per decompressed chunk (~1 MiB), so a user's
/// cancel click takes effect within milliseconds even on a multi-hundred-MB
/// dump; returns [`TomeError::Cancelled`] so the caller can distinguish
/// "user changed their mind" from a parse failure.
pub fn parse_file<F: FnMut(Redirect)>(
    path: &Path,
    cancel: &AtomicBool,
    mut on_redirect: F,
) -> Result<u64> {
    let file = File::open(path)
        .map_err(|e| TomeError::Other(format!("open redirect dump {path:?}: {e}")))?;
    // Stream the gzip in bounded chunks instead of decompressing the whole
    // dump into one String. `pending` holds only the unparsed tail — at most
    // one in-flight INSERT statement — so resident memory is independent of
    // dump size (hundreds of MB for enwiki redirects). An INSERT statement or
    // tuple that straddles a chunk boundary stays in `pending` and is
    // completed once the next chunk is read, so we don't rely on the dump
    // being newline-framed.
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
            .map_err(|e| TomeError::Other(format!("decompress redirect dump: {e}")))?;
        if n == 0 {
            // Clean EOF: flush the remainder, allowing a final statement that
            // isn't terminated by a trailing newline.
            count += parse_buf(&pending, true, &mut on_redirect).0;
            break;
        }
        pending.extend_from_slice(&chunk[..n]);
        let (parsed, consumed) = parse_buf(&pending, false, &mut on_redirect);
        count += parsed;
        pending.drain(..consumed);
    }
    Ok(count)
}

/// Parse a complete, in-memory SQL string. Retained for the unit tests that
/// feed small `&str` fixtures; `parse_file` drives the same core incrementally
/// via [`parse_buf`].
pub fn parse_str<F: FnMut(Redirect)>(content: &str, mut on_redirect: F) -> Result<u64> {
    Ok(parse_buf(content.as_bytes(), true, &mut on_redirect).0)
}

/// Parse every complete `INSERT INTO ... VALUES (...);` statement present in
/// `buf`, invoking `on_redirect` for each row.
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
fn parse_buf<F: FnMut(Redirect)>(buf: &[u8], eof: bool, on_redirect: &mut F) -> (u64, usize) {
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
        let mut rows: Vec<Redirect> = Vec::new();
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
            if let Some(r) = fields_to_redirect(&fields) {
                rows.push(r);
            }
        };
        for r in rows {
            on_redirect(r);
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

fn fields_to_redirect(fields: &[String]) -> Option<Redirect> {
    if fields.len() < 4 {
        return None;
    }
    let from_page_id: u64 = fields[0].parse().ok()?;
    let namespace: i64 = fields[1].parse().ok()?;
    if namespace != 0 {
        return None;
    }
    // rd_interwiki must be empty (no cross-wiki redirects).
    if !fields[3].is_empty() {
        return None;
    }
    let target_title = fields[2].replace('_', " ");
    let target_title = target_title.trim().to_string();
    if target_title.is_empty() {
        return None;
    }
    Some(Redirect {
        from_page_id,
        target_title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_insert_with_two_rows() {
        // rd_fragment lives in column 5; column 4 is rd_interwiki and must
        // be empty for a local redirect.
        let sql = "\
INSERT INTO `redirect` VALUES \
(123,0,'United_States','',''),\
(456,0,'Photon','','Section');";
        let mut redirects = Vec::new();
        let n = parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(redirects[0].from_page_id, 123);
        assert_eq!(redirects[0].target_title, "United States");
        assert_eq!(redirects[1].from_page_id, 456);
        assert_eq!(redirects[1].target_title, "Photon");
    }

    #[test]
    fn skips_non_main_namespace() {
        let sql = "INSERT INTO `redirect` VALUES (789,1,'Some_Talk_Page','','');";
        let mut redirects = Vec::new();
        let n = parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(n, 0);
        assert!(redirects.is_empty());
    }

    #[test]
    fn skips_interwiki_redirects() {
        let sql = "INSERT INTO `redirect` VALUES (1,0,'Some_Page','enwiki','');";
        let mut redirects = Vec::new();
        let n = parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn normalizes_underscores_to_spaces() {
        let sql = "INSERT INTO `redirect` VALUES (1,0,'United_States_of_America','','');";
        let mut redirects = Vec::new();
        parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(redirects[0].target_title, "United States of America");
    }

    #[test]
    fn handles_apostrophe_in_title() {
        let sql = "INSERT INTO `redirect` VALUES (1,0,'Joan_d\\'Arc','','');";
        let mut redirects = Vec::new();
        parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(redirects[0].target_title, "Joan d'Arc");
    }

    #[test]
    fn skips_empty_target_title() {
        let sql = "INSERT INTO `redirect` VALUES (1,0,'','','');";
        let mut redirects = Vec::new();
        let n = parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn ignores_other_inserts() {
        let sql = "INSERT INTO `something_else` VALUES (1,2,3); INSERT INTO `redirect` VALUES (42,0,'Target_Page','','');";
        let mut redirects = Vec::new();
        parse_str(sql, |r| redirects.push(r)).unwrap();
        assert_eq!(redirects.len(), 1);
        assert_eq!(redirects[0].from_page_id, 42);
        assert_eq!(redirects[0].target_title, "Target Page");
    }

    #[test]
    fn handles_statements_split_across_chunk_boundaries() {
        // Feed the SQL one byte at a time through `parse_buf`, exactly as
        // `parse_file` drives it, so every prefix, tuple, and `;` straddles a
        // "chunk" boundary. The retain-and-re-parse path must yield the same
        // rows as parsing the whole buffer at once, each row exactly once.
        let sql = "\
INSERT INTO `redirect` VALUES \
(1,0,'United_States','',''),\
(2,0,'Photon','','Section');\n\
INSERT INTO `redirect` VALUES (3,0,'Joan_d\\'Arc','','');";

        let mut streamed = Vec::new();
        {
            let mut pending: Vec<u8> = Vec::new();
            let mut push = |r: Redirect| streamed.push(r);
            for &b in sql.as_bytes() {
                pending.push(b);
                let (_, consumed) = parse_buf(&pending, false, &mut push);
                pending.drain(..consumed);
            }
            let _ = parse_buf(&pending, true, &mut push);
        }

        let mut whole = Vec::new();
        parse_str(sql, |r| whole.push(r)).unwrap();

        assert_eq!(streamed.len(), 3);
        assert_eq!(streamed.len(), whole.len());
        assert_eq!(streamed[0].target_title, "United States");
        assert_eq!(streamed[2].target_title, "Joan d'Arc");
    }
}
