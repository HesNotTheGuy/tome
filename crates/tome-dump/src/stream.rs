//! Single-stream extraction from a multistream bz2 dump.
//!
//! Wikipedia's multistream dump is a concatenation of independent bz2 streams.
//! Given a byte offset (from the index), we open the file, seek to that
//! offset, and decompress exactly one stream — never the whole dump.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use tome_core::{Result, TomeError};

pub struct DumpReader {
    path: PathBuf,
}

impl DumpReader {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Decompress the bz2 stream that begins at `offset`.
    ///
    /// `length` is optional. If provided, the read is bounded to that many
    /// bytes — recommended in production to avoid runaway reads if the file
    /// is truncated. If `None`, the bz2 decoder reads until the end of the
    /// stream marker, then stops.
    pub fn read_stream(&self, offset: u64, length: Option<u64>) -> Result<Vec<u8>> {
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;

        let mut decompressed = Vec::new();
        match length {
            Some(len) => {
                let bounded = (&mut file).take(len);
                BzDecoder::new(bounded).read_to_end(&mut decompressed)?;
            }
            None => {
                BzDecoder::new(&mut file).read_to_end(&mut decompressed)?;
            }
        }
        Ok(decompressed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPage {
    pub page_id: u64,
    pub title: String,
    pub revision_id: u64,
    pub wikitext: String,
}

/// Parse one or more `<page>` records out of a decompressed stream's bytes.
///
/// A multistream "stream" contains a sequence of `<page>` elements without a
/// wrapping root, so we wrap them before handing to the XML reader.
pub fn parse_pages(stream_bytes: &[u8]) -> Result<Vec<RawPage>> {
    let xml = std::str::from_utf8(stream_bytes)
        .map_err(|e| TomeError::Dump(format!("stream is not valid utf-8: {e}")))?;
    let wrapped = format!("<root>{xml}</root>");

    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().trim_text(false);

    let mut pages = Vec::new();
    let mut current: Option<PageBuilder> = None;
    let mut state = State::Outside;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                match (state, tag) {
                    (State::Outside, "page") => {
                        current = Some(PageBuilder::default());
                        state = State::InPage;
                    }
                    (State::InPage, "title") => state = State::InTitle,
                    (State::InPage, "id") => state = State::InPageId,
                    (State::InPage, "revision") => state = State::InRevision,
                    (State::InRevision, "id") => state = State::InRevisionId,
                    (State::InRevision, "text") => state = State::InText,
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
                match (state, tag) {
                    (State::InPage, "page") => {
                        if let Some(builder) = current.take() {
                            pages.push(builder.build()?);
                        }
                        state = State::Outside;
                    }
                    (State::InTitle, "title") => state = State::InPage,
                    (State::InPageId, "id") => state = State::InPage,
                    (State::InRevision, "revision") => state = State::InPage,
                    (State::InRevisionId, "id") => state = State::InRevision,
                    (State::InText, "text") => state = State::InRevision,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .unescape()
                    .map_err(|err| TomeError::Dump(format!("xml unescape: {err}")))?
                    .into_owned();
                if let Some(builder) = current.as_mut() {
                    match state {
                        State::InTitle => builder.title.push_str(&text),
                        State::InPageId => builder.page_id.push_str(text.trim()),
                        State::InRevisionId => builder.revision_id.push_str(text.trim()),
                        State::InText => builder.wikitext.push_str(&text),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(e)) => {
                let bytes = e.into_inner();
                let text = std::str::from_utf8(&bytes)
                    .map_err(|err| TomeError::Dump(format!("cdata utf-8: {err}")))?;
                if let Some(builder) = current.as_mut()
                    && matches!(state, State::InText)
                {
                    builder.wikitext.push_str(text);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(TomeError::Dump(format!("xml parse: {e}"))),
        }
        buf.clear();
    }

    Ok(pages)
}

#[derive(Debug, Clone, Copy)]
enum State {
    Outside,
    InPage,
    InTitle,
    InPageId,
    InRevision,
    InRevisionId,
    InText,
}

#[derive(Debug, Default)]
struct PageBuilder {
    title: String,
    page_id: String,
    revision_id: String,
    wikitext: String,
}

impl PageBuilder {
    fn build(self) -> Result<RawPage> {
        let page_id = self
            .page_id
            .parse::<u64>()
            .map_err(|e| TomeError::Dump(format!("bad page id '{}': {e}", self.page_id)))?;
        let revision_id = self
            .revision_id
            .parse::<u64>()
            .map_err(|e| TomeError::Dump(format!("bad revision id '{}': {e}", self.revision_id)))?;
        Ok(RawPage {
            page_id,
            title: self.title,
            revision_id,
            wikitext: self.wikitext,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_PAGES_XML: &[u8] = br#"
<page>
    <title>Photon</title>
    <id>23535</id>
    <revision>
        <id>987654321</id>
        <text xml:space="preserve">A '''photon''' is an [[elementary particle]].</text>
    </revision>
</page>
<page>
    <title>Electron</title>
    <id>9404</id>
    <revision>
        <id>111222333</id>
        <text xml:space="preserve">An '''electron''' is a subatomic particle.</text>
    </revision>
</page>
"#;

    #[test]
    fn parses_two_pages() {
        let pages = parse_pages(TWO_PAGES_XML).unwrap();
        assert_eq!(pages.len(), 2);

        assert_eq!(pages[0].title, "Photon");
        assert_eq!(pages[0].page_id, 23535);
        assert_eq!(pages[0].revision_id, 987654321);
        assert!(pages[0].wikitext.contains("'''photon'''"));
        assert!(pages[0].wikitext.contains("[[elementary particle]]"));

        assert_eq!(pages[1].title, "Electron");
        assert_eq!(pages[1].page_id, 9404);
        assert_eq!(pages[1].revision_id, 111222333);
    }

    #[test]
    fn handles_html_entities_in_text() {
        let xml = br#"
<page>
    <title>Test &amp; Title</title>
    <id>1</id>
    <revision>
        <id>2</id>
        <text>Foo &amp; bar &lt;baz&gt;</text>
    </revision>
</page>
"#;
        let pages = parse_pages(xml).unwrap();
        assert_eq!(pages[0].title, "Test & Title");
        assert_eq!(pages[0].wikitext, "Foo & bar <baz>");
    }

    #[test]
    fn empty_stream_yields_no_pages() {
        let pages = parse_pages(b"").unwrap();
        assert!(pages.is_empty());
    }
}
