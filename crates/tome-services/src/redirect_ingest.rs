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
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use tome_core::{Result, TomeError};
use tome_storage::Redirect;

const INSERT_PREFIX: &str = "INSERT INTO `redirect` VALUES ";

pub fn parse_file<F: FnMut(Redirect)>(path: &Path, on_redirect: F) -> Result<u64> {
    let file = File::open(path)
        .map_err(|e| TomeError::Other(format!("open redirect dump {path:?}: {e}")))?;
    let mut gz = GzDecoder::new(file);
    let mut content = String::new();
    gz.read_to_string(&mut content)
        .map_err(|e| TomeError::Other(format!("decompress redirect dump: {e}")))?;
    parse_str(&content, on_redirect)
}

pub fn parse_str<F: FnMut(Redirect)>(content: &str, mut on_redirect: F) -> Result<u64> {
    let mut count: u64 = 0;
    for (pos, _) in content.match_indices(INSERT_PREFIX) {
        let after = &content[pos + INSERT_PREFIX.len()..];
        let bytes = after.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i] != b';' {
            while i < bytes.len() && bytes[i] != b'(' && bytes[i] != b';' {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b';' {
                break;
            }
            i += 1; // consume '('
            let (fields, advance) = parse_tuple(&after[i..])?;
            i += advance;
            if let Some(r) = fields_to_redirect(&fields) {
                on_redirect(r);
                count += 1;
            }
        }
    }
    Ok(count)
}

fn parse_tuple(input: &str) -> Result<(Vec<String>, usize)> {
    let bytes = input.as_bytes();
    let mut fields: Vec<String> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escape = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
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
                return Ok((fields, i));
            }
            _ => current.push(b),
        }
    }
    Err(TomeError::Other("unterminated redirect tuple".into()))
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
}
