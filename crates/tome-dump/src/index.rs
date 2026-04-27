//! Streaming reader for `*-multistream-index.txt.bz2`.
//!
//! The file is bz2-compressed; each decompressed line has the form
//! `offset:page_id:title`. We stream-decompress and yield entries one at a
//! time so the entire index never needs to live in memory.

use std::io::{BufRead, BufReader, Read};

use bzip2::read::MultiBzDecoder;
use tome_core::{Result, TomeError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub stream_offset: u64,
    pub page_id: u64,
    pub title: String,
}

pub struct IndexReader<R: Read> {
    inner: BufReader<MultiBzDecoder<R>>,
    line_buf: String,
}

impl<R: Read> IndexReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            inner: BufReader::new(MultiBzDecoder::new(reader)),
            line_buf: String::new(),
        }
    }
}

impl<R: Read> Iterator for IndexReader<R> {
    type Item = Result<IndexEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.line_buf.clear();
        match self.inner.read_line(&mut self.line_buf) {
            Ok(0) => None,
            Ok(_) => {
                let trimmed = self.line_buf.trim_end_matches(['\r', '\n']);
                if trimmed.is_empty() {
                    return self.next();
                }
                Some(parse_index_line(trimmed))
            }
            Err(e) => Some(Err(e.into())),
        }
    }
}

pub fn parse_index_line(line: &str) -> Result<IndexEntry> {
    let mut parts = line.splitn(3, ':');
    let offset = parts
        .next()
        .ok_or_else(|| TomeError::Dump(format!("missing offset in: {line}")))?;
    let page_id = parts
        .next()
        .ok_or_else(|| TomeError::Dump(format!("missing page_id in: {line}")))?;
    let title = parts
        .next()
        .ok_or_else(|| TomeError::Dump(format!("missing title in: {line}")))?;

    let stream_offset = offset
        .parse::<u64>()
        .map_err(|e| TomeError::Dump(format!("bad offset '{offset}': {e}")))?;
    let page_id = page_id
        .parse::<u64>()
        .map_err(|e| TomeError::Dump(format!("bad page_id '{page_id}': {e}")))?;

    Ok(IndexEntry {
        stream_offset,
        page_id,
        title: title.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_line() {
        let entry = parse_index_line("12345:678:Photon").unwrap();
        assert_eq!(entry.stream_offset, 12345);
        assert_eq!(entry.page_id, 678);
        assert_eq!(entry.title, "Photon");
    }

    #[test]
    fn title_may_contain_colons() {
        // Wikipedia article titles can include colons (e.g. "User:Foo", "Help:Contents").
        let entry = parse_index_line("12345:678:Help:Contents").unwrap();
        assert_eq!(entry.title, "Help:Contents");
    }

    #[test]
    fn rejects_malformed_offset() {
        let err = parse_index_line("notanumber:678:Photon").unwrap_err();
        assert!(matches!(err, TomeError::Dump(_)));
    }

    #[test]
    fn rejects_malformed_page_id() {
        let err = parse_index_line("12345:notanumber:Photon").unwrap_err();
        assert!(matches!(err, TomeError::Dump(_)));
    }

    #[test]
    fn rejects_too_few_fields() {
        let err = parse_index_line("12345:678").unwrap_err();
        assert!(matches!(err, TomeError::Dump(_)));
    }
}
