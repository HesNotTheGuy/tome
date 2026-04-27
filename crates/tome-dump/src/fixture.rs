//! Programmatic fixture builder for tests.
//!
//! Produces an in-memory pair of (multistream dump, index) bytes from a list
//! of synthetic pages, so dump-pipeline tests can run without any real
//! Wikipedia data on disk.

use std::io::Write;

use bzip2::Compression;
use bzip2::write::BzEncoder;
use tome_core::{Result, TomeError};

#[derive(Debug, Clone)]
pub struct PageData {
    pub title: String,
    pub page_id: u64,
    pub revision_id: u64,
    pub wikitext: String,
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub stream_offset: u64,
    pub stream_length: u64,
    pub pages: Vec<PageData>,
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub dump_bytes: Vec<u8>,
    pub index_bytes: Vec<u8>,
    pub streams: Vec<StreamInfo>,
}

/// Build a multistream fixture. Pages are grouped into chunks of
/// `pages_per_stream`; each chunk becomes one bz2 stream. The index is itself
/// bz2-compressed (a single stream).
pub fn build_fixture(pages: Vec<PageData>, pages_per_stream: usize) -> Result<Fixture> {
    if pages_per_stream == 0 {
        return Err(TomeError::Other("pages_per_stream must be > 0".into()));
    }

    let mut dump_bytes = Vec::new();
    let mut streams = Vec::new();
    let mut index_lines = String::new();

    for chunk in pages.chunks(pages_per_stream) {
        let stream_offset = dump_bytes.len() as u64;
        let stream_start = dump_bytes.len();

        let mut xml = String::new();
        for page in chunk {
            xml.push_str(&format_page_xml(page));
        }

        {
            let mut encoder = BzEncoder::new(&mut dump_bytes, Compression::default());
            encoder.write_all(xml.as_bytes())?;
            encoder.finish()?;
        }

        let stream_length = (dump_bytes.len() - stream_start) as u64;

        for page in chunk {
            index_lines.push_str(&format!(
                "{}:{}:{}\n",
                stream_offset, page.page_id, page.title
            ));
        }

        streams.push(StreamInfo {
            stream_offset,
            stream_length,
            pages: chunk.to_vec(),
        });
    }

    let mut index_bytes = Vec::new();
    {
        let mut encoder = BzEncoder::new(&mut index_bytes, Compression::default());
        encoder.write_all(index_lines.as_bytes())?;
        encoder.finish()?;
    }

    Ok(Fixture {
        dump_bytes,
        index_bytes,
        streams,
    })
}

fn format_page_xml(page: &PageData) -> String {
    format!(
        "<page>\n<title>{}</title>\n<id>{}</id>\n<revision>\n<id>{}</id>\n\
         <text xml:space=\"preserve\">{}</text>\n</revision>\n</page>\n",
        xml_escape(&page.title),
        page.page_id,
        page.revision_id,
        xml_escape(&page.wikitext)
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pages() -> Vec<PageData> {
        vec![
            PageData {
                title: "Photon".into(),
                page_id: 23535,
                revision_id: 987654321,
                wikitext: "A '''photon''' is an [[elementary particle]].".into(),
            },
            PageData {
                title: "Electron".into(),
                page_id: 9404,
                revision_id: 111222333,
                wikitext: "An '''electron''' is a subatomic particle.".into(),
            },
            PageData {
                title: "Quark".into(),
                page_id: 25313,
                revision_id: 444555666,
                wikitext: "A '''quark''' is an elementary particle.".into(),
            },
        ]
    }

    #[test]
    fn fixture_groups_pages_into_streams() {
        let fx = build_fixture(sample_pages(), 2).unwrap();
        assert_eq!(fx.streams.len(), 2);
        assert_eq!(fx.streams[0].pages.len(), 2);
        assert_eq!(fx.streams[1].pages.len(), 1);
    }

    #[test]
    fn fixture_offsets_match_dump_layout() {
        let fx = build_fixture(sample_pages(), 1).unwrap();
        assert_eq!(fx.streams.len(), 3);
        assert_eq!(fx.streams[0].stream_offset, 0);
        assert_eq!(fx.streams[1].stream_offset, fx.streams[0].stream_length);
        assert_eq!(
            fx.streams[2].stream_offset,
            fx.streams[1].stream_offset + fx.streams[1].stream_length
        );
        assert_eq!(
            fx.dump_bytes.len() as u64,
            fx.streams[2].stream_offset + fx.streams[2].stream_length
        );
    }

    #[test]
    fn rejects_zero_pages_per_stream() {
        let err = build_fixture(sample_pages(), 0).unwrap_err();
        assert!(matches!(err, TomeError::Other(_)));
    }
}
