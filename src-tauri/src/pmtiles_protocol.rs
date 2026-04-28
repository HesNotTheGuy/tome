//! Custom URI scheme `tome-pmtiles://localhost/` that streams byte ranges out
//! of the user's configured `.pmtiles` archive.
//!
//! The Map pane (MapLibre + the `pmtiles` JS library) issues HEAD then ranged
//! GET requests against this scheme; we open the file read-only, seek to the
//! requested byte window, and return the slice. The full file never enters
//! memory.
//!
//! # Data-integrity contract
//!
//! This handler is one of the file-reading paths covered by the
//! `file_integrity` invariant: the user's `.pmtiles` file is opened with
//! [`std::fs::File::open`] (read-only) and never touched any other way. A
//! future change that mutates this code path must be paired with a test in
//! `tome-services/tests/file_integrity.rs` for the pmtiles scheme.

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use tauri::http;
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};
use tome_services::Tome;

pub const SCHEME: &str = "tome-pmtiles";

/// Spawn a worker thread to handle one request and resolve `responder` with
/// the result. Used as the body of `register_asynchronous_uri_scheme_protocol`.
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    std::thread::spawn(move || {
        let response = serve(&app, &request);
        responder.respond(response);
    });
}

fn serve<R: Runtime>(
    app: &AppHandle<R>,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Cow<'static, [u8]>> {
    let Some(state) = app.try_state::<Arc<Tome>>() else {
        return error(503, "tome not yet ready");
    };
    let Some(path) = state.inner().map_source_path() else {
        return error(404, "no map source configured");
    };

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => return error(404, &format!("open {}: {}", path.display(), e)),
    };
    let total_len = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => return error(500, &format!("metadata: {e}")),
    };

    let method = request.method().as_str();
    let range_header = request.headers().get(http::header::RANGE);

    if method != "GET" && method != "HEAD" {
        return error(405, "only GET and HEAD are supported");
    }

    let (start, end_inclusive) = match parse_range(range_header, total_len) {
        Ok(r) => r,
        Err(msg) => return error(416, msg),
    };
    let len = end_inclusive - start + 1;

    let body: Cow<'static, [u8]> = if method == "HEAD" {
        Cow::Borrowed(&[])
    } else {
        match read_window(file, start, len) {
            Ok(v) => Cow::Owned(v),
            Err(e) => return error(500, &format!("read: {e}")),
        }
    };

    let status = if range_header.is_some() { 206 } else { 200 };
    let mut builder = http::Response::builder()
        .status(status)
        .header("Content-Type", "application/octet-stream")
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .header("Content-Length", len.to_string());
    if range_header.is_some() {
        builder = builder.header(
            "Content-Range",
            format!("bytes {start}-{end_inclusive}/{total_len}"),
        );
    }
    builder.body(body).expect("build response")
}

fn read_window(mut file: File, start: u64, len: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Parse an HTTP `Range` header into an inclusive `(start, end)` byte window.
///
/// Supported forms:
///   `bytes=N-M` — explicit window
///   `bytes=N-`  — from N to end
///   `bytes=-N`  — last N bytes (suffix)
///
/// Multi-range and non-`bytes` units return `Err`. If the header is absent,
/// the whole file is selected.
fn parse_range(
    header: Option<&http::HeaderValue>,
    total_len: u64,
) -> Result<(u64, u64), &'static str> {
    if total_len == 0 {
        return Err("file is empty");
    }
    let Some(h) = header else {
        return Ok((0, total_len - 1));
    };
    let s = h.to_str().map_err(|_| "Range header is not utf-8")?;
    let s = s
        .strip_prefix("bytes=")
        .ok_or("Range must use bytes= unit")?;
    if s.contains(',') {
        return Err("multi-range not supported");
    }
    let (lo, hi) = s.split_once('-').ok_or("missing '-' in Range")?;

    let (start, end_inclusive): (u64, u64) = if lo.is_empty() {
        let n: u64 = hi.parse().map_err(|_| "invalid suffix length")?;
        if n == 0 {
            return Err("suffix length must be > 0");
        }
        let n = n.min(total_len);
        (total_len - n, total_len - 1)
    } else {
        let start: u64 = lo.parse().map_err(|_| "invalid range start")?;
        let end: u64 = if hi.is_empty() {
            total_len - 1
        } else {
            let raw: u64 = hi.parse().map_err(|_| "invalid range end")?;
            raw.min(total_len - 1)
        };
        (start, end)
    };

    if start > end_inclusive || start >= total_len {
        return Err("range out of bounds");
    }
    Ok((start, end_inclusive))
}

fn error(status: u16, message: &str) -> http::Response<Cow<'static, [u8]>> {
    http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Access-Control-Allow-Origin", "*")
        .body(Cow::Owned(message.as_bytes().to_vec()))
        .expect("build error response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn hv(s: &str) -> HeaderValue {
        HeaderValue::from_str(s).unwrap()
    }

    #[test]
    fn no_range_means_whole_file() {
        assert_eq!(parse_range(None, 100).unwrap(), (0, 99));
    }

    #[test]
    fn explicit_range() {
        let h = hv("bytes=10-20");
        assert_eq!(parse_range(Some(&h), 100).unwrap(), (10, 20));
    }

    #[test]
    fn open_ended_range_reads_to_end() {
        let h = hv("bytes=50-");
        assert_eq!(parse_range(Some(&h), 100).unwrap(), (50, 99));
    }

    #[test]
    fn suffix_range_reads_last_n_bytes() {
        let h = hv("bytes=-25");
        assert_eq!(parse_range(Some(&h), 100).unwrap(), (75, 99));
    }

    #[test]
    fn end_clamps_to_file_length() {
        let h = hv("bytes=90-200");
        assert_eq!(parse_range(Some(&h), 100).unwrap(), (90, 99));
    }

    #[test]
    fn out_of_bounds_start_errors() {
        let h = hv("bytes=200-300");
        assert!(parse_range(Some(&h), 100).is_err());
    }

    #[test]
    fn multi_range_rejected() {
        let h = hv("bytes=0-10,20-30");
        assert!(parse_range(Some(&h), 100).is_err());
    }

    #[test]
    fn non_bytes_unit_rejected() {
        let h = hv("rows=0-10");
        assert!(parse_range(Some(&h), 100).is_err());
    }

    /// Data-integrity contract: serving a byte window from a user file must
    /// never modify it. Sets the file read-only on disk so a buggy write would
    /// fail with EACCES/ERROR_ACCESS_DENIED instead of silently mutating.
    #[test]
    fn serving_a_byte_window_does_not_mutate_source_file() {
        let mut f = NamedTempFile::new().unwrap();
        let payload = (0u8..=255).cycle().take(4096).collect::<Vec<u8>>();
        f.write_all(&payload).unwrap();
        f.flush().unwrap();

        let before = fs::read(f.path()).unwrap();
        let mut perms = fs::metadata(f.path()).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(f.path(), perms).unwrap();

        // Repeatedly serve different windows, mimicking what MapLibre does as
        // the user pans across tiles. Use the same private read_window helper
        // the live request handler uses, so this is the actual code path
        // exercised in production.
        for (start, len) in [(0u64, 32u64), (100, 256), (1024, 1024), (3000, 96)] {
            let file = File::open(f.path()).expect("open");
            let bytes = read_window(file, start, len).expect("read");
            assert_eq!(bytes.len(), len as usize);
            let s = start as usize;
            let e = s + len as usize;
            assert_eq!(bytes, &payload[s..e]);
        }

        let mut perms = fs::metadata(f.path()).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(f.path(), perms).unwrap();

        let after = fs::read(f.path()).unwrap();
        assert_eq!(before, after, "pmtiles source file was mutated");
    }
}
