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
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use tome_core::{Result, TomeError};
use tome_storage::Geotag;

const INSERT_PREFIX: &str = "INSERT INTO `geo_tags` VALUES ";

pub fn parse_file<F: FnMut(Geotag)>(path: &Path, mut on_geotag: F) -> Result<u64> {
    let file = File::open(path)
        .map_err(|e| TomeError::Other(format!("open geotag dump {path:?}: {e}")))?;
    let mut gz = GzDecoder::new(file);
    let mut content = String::new();
    gz.read_to_string(&mut content)
        .map_err(|e| TomeError::Other(format!("decompress geotag dump: {e}")))?;
    parse_str(&content, |g| on_geotag(g))
}

pub fn parse_str<F: FnMut(Geotag)>(content: &str, mut on_geotag: F) -> Result<u64> {
    let mut count: u64 = 0;
    for (pos, _) in content.match_indices(INSERT_PREFIX) {
        let after = &content[pos + INSERT_PREFIX.len()..];
        let bytes = after.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i] != b';' {
            // Skip to next '('
            while i < bytes.len() && bytes[i] != b'(' && bytes[i] != b';' {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b';' {
                break;
            }
            i += 1; // consume '('
            let (fields, advance) = parse_tuple(&after[i..])?;
            i += advance;
            if let Some(g) = fields_to_geotag(&fields) {
                on_geotag(g);
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
            b',' => {
                fields.push(take_field(&mut current));
            }
            b')' => {
                fields.push(take_field(&mut current));
                return Ok((fields, i));
            }
            _ => current.push(b),
        }
    }
    Err(TomeError::Other("unterminated geotag tuple".into()))
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
        let sql = "INSERT INTO `geo_tags` VALUES (1,1,'earth',1,999.0,500.0,0,'broken',NULL,NULL,NULL);";
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
}
