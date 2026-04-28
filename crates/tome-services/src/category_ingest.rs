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
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use tome_core::{Result, TomeError};
use tome_storage::{CategoryLink, CategoryMemberKind};

const INSERT_PREFIX: &str = "INSERT INTO `categorylinks` VALUES ";

pub fn parse_file<F: FnMut(CategoryLink)>(path: &Path, on_link: F) -> Result<u64> {
    let file = File::open(path)
        .map_err(|e| TomeError::Other(format!("open categorylinks dump {path:?}: {e}")))?;
    let mut gz = GzDecoder::new(file);
    let mut content = String::new();
    gz.read_to_string(&mut content)
        .map_err(|e| TomeError::Other(format!("decompress categorylinks dump: {e}")))?;
    parse_str(&content, on_link)
}

pub fn parse_str<F: FnMut(CategoryLink)>(content: &str, mut on_link: F) -> Result<u64> {
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
            if let Some(link) = fields_to_link(&fields) {
                on_link(link);
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
    Err(TomeError::Other("unterminated categorylinks tuple".into()))
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
}
