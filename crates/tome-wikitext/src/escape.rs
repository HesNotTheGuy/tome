//! HTML escaping for safely embedding wikitext-derived content.

/// Escape the five characters that have special meaning in HTML body text.
pub fn escape_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Escape for HTML attribute values (single line, double-quoted).
pub fn escape_attr(input: &str) -> String {
    escape_text(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_for_plain_text() {
        assert_eq!(escape_text("Hello world"), "Hello world");
    }

    #[test]
    fn escapes_ampersand() {
        assert_eq!(escape_text("R&D"), "R&amp;D");
    }

    #[test]
    fn escapes_angle_brackets_and_quotes() {
        assert_eq!(
            escape_text("<a href=\"x\">'y'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&#39;y&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn unicode_passes_through() {
        assert_eq!(escape_text("café — résumé"), "café — résumé");
    }
}
