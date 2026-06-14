//! Wikitext-AST to HTML walker.
//!
//! Bold and italic are toggle nodes in the AST (no children) — we track state
//! across siblings and emit balanced open/close tags. Paragraphs are inferred
//! from the layout: inline content auto-opens a `<p>`, block elements
//! (headings, lists, horizontal rules) close the current paragraph,
//! `ParagraphBreak` does the same.

use std::collections::HashMap;

use parse_wiki_text_2::{Configuration, Node, Parameter};

use crate::escape::{escape_attr, escape_text};
use crate::link::{LinkResolver, LinkStatus};

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Headings below this level (h1) are bumped up. Article titles are
    /// rendered by the UI shell as h1, so wikitext `=Section=` becomes h2 by
    /// default to avoid collisions.
    pub min_heading_level: u8,
    /// Headings above this level are clamped to it. HTML supports h1-h6.
    pub max_heading_level: u8,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            min_heading_level: 2,
            max_heading_level: 6,
        }
    }
}

pub struct Renderer {
    options: RenderOptions,
    resolver: Box<dyn LinkResolver>,
}

impl Renderer {
    pub fn new(resolver: Box<dyn LinkResolver>) -> Self {
        Self {
            options: RenderOptions::default(),
            resolver,
        }
    }

    pub fn with_options(mut self, options: RenderOptions) -> Self {
        self.options = options;
        self
    }

    /// Render wikitext to HTML. On parser failure, returns a `<pre>` block
    /// containing the raw wikitext with a banner — never blank, never a
    /// stack trace.
    pub fn render(&self, wikitext: &str) -> String {
        let parsed = match Configuration::default().parse(wikitext) {
            Ok(out) => out,
            Err(err) => {
                return format!(
                    "<div class=\"tome-render-error\">Parser error: {}</div>\
                     <pre class=\"tome-raw\">{}</pre>",
                    escape_text(&format!("{err:?}")),
                    escape_text(wikitext)
                );
            }
        };

        let mut state = State::new(&*self.resolver, &self.options);
        walk(&mut state, &parsed.nodes);
        state.close_inline();
        state.close_paragraph();
        state.flush_refs();
        state.out
    }
}

struct State<'a> {
    out: String,
    bold: bool,
    italic: bool,
    bold_italic: bool,
    in_paragraph: bool,
    refs: Vec<String>,
    /// Per-document heading slug usage, for deduplicating anchor ids.
    /// Reset with each `render` call (this struct is built per call).
    heading_counts: HashMap<String, u32>,
    resolver: &'a dyn LinkResolver,
    options: &'a RenderOptions,
}

impl<'a> State<'a> {
    fn new(resolver: &'a dyn LinkResolver, options: &'a RenderOptions) -> Self {
        Self {
            out: String::new(),
            bold: false,
            italic: false,
            bold_italic: false,
            in_paragraph: false,
            refs: Vec::new(),
            heading_counts: HashMap::new(),
            resolver,
            options,
        }
    }

    /// Stable anchor id for a heading: `"s-" + slug`, with `-2`, `-3`, …
    /// suffixes for repeated slugs in document order. The Reader UI builds
    /// its table of contents from these ids — the format is a fixed contract.
    fn heading_id(&mut self, text: &str) -> String {
        let slug = slugify(text);
        let count = self.heading_counts.entry(slug.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            format!("s-{slug}")
        } else {
            format!("s-{slug}-{count}")
        }
    }

    fn ensure_paragraph_open(&mut self) {
        if !self.in_paragraph {
            self.out.push_str("<p>");
            self.in_paragraph = true;
        }
    }

    /// Close any unclosed inline emphasis tags. Called before block boundaries
    /// so a bold/italic that didn't get closed by the source still produces
    /// well-formed HTML.
    fn close_inline(&mut self) {
        if self.bold_italic {
            self.out.push_str("</em></strong>");
            self.bold_italic = false;
        }
        if self.bold {
            self.out.push_str("</strong>");
            self.bold = false;
        }
        if self.italic {
            self.out.push_str("</em>");
            self.italic = false;
        }
    }

    fn close_paragraph(&mut self) {
        if self.in_paragraph {
            self.out.push_str("</p>");
            self.in_paragraph = false;
        }
    }

    fn flush_refs(&mut self) {
        if self.refs.is_empty() {
            return;
        }
        // The synthesized References heading participates in the same anchor
        // namespace as article headings, so an explicit `== References ==`
        // earlier in the document still yields unique ids.
        let id = self.heading_id("References");
        self.out.push_str(&format!(
            "<section class=\"tome-references\"><h2 id=\"{}\">References</h2><ol>",
            escape_attr(&id)
        ));
        for (i, content) in self.refs.iter().enumerate() {
            let n = i + 1;
            self.out
                .push_str(&format!("<li id=\"ref-{n}\">{content}</li>"));
        }
        self.out.push_str("</ol></section>");
    }
}

fn walk(state: &mut State<'_>, nodes: &[Node<'_>]) {
    for node in nodes {
        render_node(state, node);
    }
}

fn render_node(state: &mut State<'_>, node: &Node<'_>) {
    match node {
        Node::Text { value, .. } => {
            state.ensure_paragraph_open();
            state.out.push_str(&escape_text(value));
        }
        Node::CharacterEntity { character, .. } => {
            state.ensure_paragraph_open();
            state.out.push_str(&escape_text(&character.to_string()));
        }
        Node::Bold { .. } => {
            state.ensure_paragraph_open();
            if state.bold {
                state.out.push_str("</strong>");
            } else {
                state.out.push_str("<strong>");
            }
            state.bold = !state.bold;
        }
        Node::Italic { .. } => {
            state.ensure_paragraph_open();
            if state.italic {
                state.out.push_str("</em>");
            } else {
                state.out.push_str("<em>");
            }
            state.italic = !state.italic;
        }
        Node::BoldItalic { .. } => {
            state.ensure_paragraph_open();
            if state.bold_italic {
                state.out.push_str("</em></strong>");
            } else {
                state.out.push_str("<strong><em>");
            }
            state.bold_italic = !state.bold_italic;
        }
        Node::ParagraphBreak { .. } => {
            state.close_inline();
            state.close_paragraph();
        }
        Node::Heading {
            level,
            nodes: children,
            ..
        } => {
            state.close_inline();
            state.close_paragraph();
            let lvl = (*level)
                .max(state.options.min_heading_level)
                .min(state.options.max_heading_level);
            let mut heading_text = String::new();
            collect_text_into(&mut heading_text, children);
            let id = state.heading_id(&heading_text);
            state
                .out
                .push_str(&format!("<h{lvl} id=\"{}\">", escape_attr(&id)));
            let was_in_p = state.in_paragraph;
            state.in_paragraph = true; // suppress nested <p> wrapping
            walk(state, children);
            state.close_inline();
            state.in_paragraph = was_in_p;
            state.out.push_str(&format!("</h{lvl}>"));
        }
        Node::HorizontalDivider { .. } => {
            state.close_inline();
            state.close_paragraph();
            state.out.push_str("<hr/>");
        }
        Node::Link { target, text, .. } => {
            state.ensure_paragraph_open();
            let status = state.resolver.resolve_internal(target);
            let (resolved_target, class) = match &status {
                LinkStatus::Available => (target.to_string(), "tome-wikilink"),
                LinkStatus::Redirect(t) => (t.clone(), "tome-wikilink"),
                LinkStatus::Missing => (target.to_string(), "tome-wikilink tome-missing"),
            };
            state.out.push_str(&format!(
                "<a href=\"#/article/{}\" class=\"{}\">",
                escape_attr(&url_encode_title(&resolved_target)),
                class
            ));
            if text.is_empty() {
                state.out.push_str(&escape_text(target));
            } else {
                walk(state, text);
            }
            state.close_inline();
            state.out.push_str("</a>");
        }
        Node::ExternalLink {
            nodes: children, ..
        } => {
            state.ensure_paragraph_open();
            // External link nodes typically begin with a Text node containing
            // "URL [whitespace label]". Split off the first whitespace-token
            // as the URL; remainder is the label.
            let (url, label) = split_external_link(children);
            state.out.push_str(&format!(
                "<a href=\"{}\" class=\"tome-extlink\" target=\"_blank\" rel=\"noopener noreferrer\">",
                escape_attr(&url)
            ));
            if label.trim().is_empty() {
                state.out.push_str(&escape_text(&url));
            } else {
                state.out.push_str(&escape_text(label.trim()));
            }
            state.out.push_str("</a>");
        }
        Node::UnorderedList { items, .. } => {
            state.close_inline();
            state.close_paragraph();
            state.out.push_str("<ul>");
            for item in items {
                state.out.push_str("<li>");
                let was_in_p = state.in_paragraph;
                state.in_paragraph = true; // suppress <p> wrapping inside <li>
                walk(state, &item.nodes);
                state.close_inline();
                state.in_paragraph = was_in_p;
                state.out.push_str("</li>");
            }
            state.out.push_str("</ul>");
        }
        Node::OrderedList { items, .. } => {
            state.close_inline();
            state.close_paragraph();
            state.out.push_str("<ol>");
            for item in items {
                state.out.push_str("<li>");
                let was_in_p = state.in_paragraph;
                state.in_paragraph = true;
                walk(state, &item.nodes);
                state.close_inline();
                state.in_paragraph = was_in_p;
                state.out.push_str("</li>");
            }
            state.out.push_str("</ol>");
        }
        Node::DefinitionList { items, .. } => {
            state.close_inline();
            state.close_paragraph();
            state.out.push_str("<dl>");
            for item in items {
                let tag = match item.type_ {
                    parse_wiki_text_2::DefinitionListItemType::Term => "dt",
                    parse_wiki_text_2::DefinitionListItemType::Details => "dd",
                };
                state.out.push_str(&format!("<{tag}>"));
                let was_in_p = state.in_paragraph;
                state.in_paragraph = true;
                walk(state, &item.nodes);
                state.close_inline();
                state.in_paragraph = was_in_p;
                state.out.push_str(&format!("</{tag}>"));
            }
            state.out.push_str("</dl>");
        }
        Node::Template {
            name, parameters, ..
        } => {
            render_template(state, name, parameters);
        }
        Node::Image { target, .. } => {
            state.ensure_paragraph_open();
            state.out.push_str(&format!(
                "<span class=\"tome-image-placeholder\" title=\"images not fetched\">[Image: {}]</span>",
                escape_text(target)
            ));
        }
        Node::Tag {
            name,
            nodes: children,
            ..
        } => {
            let n = name.as_ref();
            if n.eq_ignore_ascii_case("ref") {
                state.ensure_paragraph_open();
                // Render the body through the full walker (not the plain-text
                // collector) so cite templates inside refs format as
                // citations and text is escaped. The output buffer and inline
                // state are swapped out so the footnote renders in isolation.
                let saved_out = std::mem::take(&mut state.out);
                let saved_in_p = state.in_paragraph;
                let saved_bold = state.bold;
                let saved_italic = state.italic;
                let saved_bold_italic = state.bold_italic;
                state.in_paragraph = true; // suppress <p> wrapping in footnotes
                state.bold = false;
                state.italic = false;
                state.bold_italic = false;
                walk(state, children);
                state.close_inline();
                let body = std::mem::replace(&mut state.out, saved_out);
                state.in_paragraph = saved_in_p;
                state.bold = saved_bold;
                state.italic = saved_italic;
                state.bold_italic = saved_bold_italic;
                state.refs.push(body);
                let idx = state.refs.len();
                state.out.push_str(&format!(
                    "<sup class=\"tome-ref\"><a href=\"#ref-{idx}\">[{idx}]</a></sup>"
                ));
            } else if n.eq_ignore_ascii_case("nowiki") {
                state.ensure_paragraph_open();
                let mut buf = String::new();
                collect_text_into(&mut buf, children);
                state.out.push_str(&escape_text(&buf));
            } else {
                // Pass the inner content through; skip the tag itself.
                walk(state, children);
            }
        }
        Node::Preformatted {
            nodes: children, ..
        } => {
            state.close_inline();
            state.close_paragraph();
            state.out.push_str("<pre class=\"tome-preformatted\">");
            let mut buf = String::new();
            collect_text_into(&mut buf, children);
            state.out.push_str(&escape_text(&buf));
            state.out.push_str("</pre>");
        }
        Node::Table { rows, captions, .. } => {
            state.close_inline();
            state.close_paragraph();
            state.out.push_str("<table class=\"tome-table\">");
            for caption in captions {
                state.out.push_str("<caption>");
                walk(state, &caption.content);
                state.close_inline();
                state.out.push_str("</caption>");
            }
            for row in rows {
                state.out.push_str("<tr>");
                for cell in &row.cells {
                    let tag = match cell.type_ {
                        parse_wiki_text_2::TableCellType::Heading => "th",
                        parse_wiki_text_2::TableCellType::Ordinary => "td",
                    };
                    state.out.push_str(&format!("<{tag}>"));
                    let was_in_p = state.in_paragraph;
                    state.in_paragraph = true;
                    walk(state, &cell.content);
                    state.close_inline();
                    state.in_paragraph = was_in_p;
                    state.out.push_str(&format!("</{tag}>"));
                }
                state.out.push_str("</tr>");
            }
            state.out.push_str("</table>");
        }
        // Silently dropped: comments, magic words, redirects, categories,
        // parameters, bare tag fragments. None render as visible content.
        Node::Comment { .. }
        | Node::MagicWord { .. }
        | Node::Redirect { .. }
        | Node::Category { .. }
        | Node::Parameter { .. }
        | Node::StartTag { .. }
        | Node::EndTag { .. } => {}
    }
}

fn collect_text_into(buf: &mut String, nodes: &[Node<'_>]) {
    for node in nodes {
        match node {
            Node::Text { value, .. } => buf.push_str(value),
            Node::CharacterEntity { character, .. } => buf.push(*character),
            Node::Heading {
                nodes: children, ..
            } => collect_text_into(buf, children),
            Node::Bold { .. } | Node::Italic { .. } | Node::BoldItalic { .. } => {
                // Toggles carry no content of their own.
            }
            Node::Link { target, text, .. } => {
                if text.is_empty() {
                    buf.push_str(target);
                } else {
                    collect_text_into(buf, text);
                }
            }
            Node::Template { name, .. } => {
                buf.push('{');
                collect_text_into(buf, name);
                buf.push('}');
            }
            Node::Tag {
                nodes: children, ..
            } => collect_text_into(buf, children),
            _ => {}
        }
    }
}

/// Encode a Wikipedia article title for use in a URL fragment.
/// Spaces become underscores (Wikipedia URL convention); a small set of
/// reserved characters are percent-encoded to keep the URL well-formed.
fn url_encode_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        match ch {
            ' ' => out.push('_'),
            '#' | '?' | '%' | '&' | '"' | '<' | '>' | '\\' | '|' | '{' | '}' => {
                let mut buf = [0u8; 4];
                for byte in ch.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Slug for heading anchors: lowercase, ASCII alphanumerics kept, every other
/// character collapsed to single `-`, leading/trailing `-` trimmed,
/// `"section"` when nothing survives.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut prev_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Render a template by family. Known families (infoboxes, citations,
/// convert, lang/IPA) get readable approximations; everything else falls
/// back to the placeholder span.
fn render_template(state: &mut State<'_>, name: &[Node<'_>], parameters: &[Parameter<'_>]) {
    let mut raw_name = String::new();
    collect_text_into(&mut raw_name, name);
    let raw_name = raw_name.trim();
    // Template names use '_' interchangeably with ' '; normalize for dispatch.
    let normalized = raw_name.to_lowercase().replace('_', " ");

    let rendered = if normalized.starts_with("infobox") {
        render_infobox(state, raw_name, parameters)
    } else if normalized == "citation" || normalized.starts_with("cite") {
        render_citation(state, parameters)
    } else if normalized == "convert" {
        render_convert(state, parameters)
    } else if normalized == "lang" {
        render_positional_text(state, parameters, PositionalPick::Last)
    } else if normalized.starts_with("ipa") {
        render_positional_text(state, parameters, PositionalPick::First)
    } else {
        false
    };

    if !rendered {
        render_template_placeholder(state, raw_name);
    }
}

fn render_template_placeholder(state: &mut State<'_>, name_text: &str) {
    state.ensure_paragraph_open();
    state.out.push_str(&format!(
        "<span class=\"tome-template-placeholder\" title=\"template not rendered locally\">[Template: {}]</span>",
        escape_text(name_text)
    ));
}

/// Infoboxes are block-level: the open paragraph closes before the div,
/// mirroring tables. Returns false (caller emits the placeholder) when no
/// named parameter has a non-empty value.
fn render_infobox(state: &mut State<'_>, raw_name: &str, parameters: &[Parameter<'_>]) -> bool {
    let mut rows = String::new();
    for param in parameters {
        let Some(name_nodes) = &param.name else {
            continue; // positional parameters carry no label to show
        };
        let key = collect_param_text(name_nodes);
        let value = collect_param_text(&param.value);
        if key.is_empty() || value.is_empty() {
            continue;
        }
        rows.push_str(&format!(
            "<tr><th>{}</th><td>{}</td></tr>",
            escape_text(&key.replace('_', " ")),
            escape_text(&value)
        ));
    }
    if rows.is_empty() {
        return false;
    }
    state.close_inline();
    state.close_paragraph();
    state.out.push_str(&format!(
        "<div class=\"tome-infobox\"><div class=\"tome-infobox-title\">{}</div>\
         <table class=\"tome-infobox-params\">{rows}</table></div>",
        escape_text(&prettify_template_name(raw_name))
    ));
    true
}

/// Citation templates render inline as "“title”. author. (date). url" with
/// each part included only when present. Returns false when none of the
/// recognized parameters exist.
fn render_citation(state: &mut State<'_>, parameters: &[Parameter<'_>]) -> bool {
    let mut parts: Vec<String> = Vec::new();
    if let Some(title) = named_param(parameters, "title") {
        parts.push(format!("\u{201C}{}\u{201D}", escape_text(&title)));
    }
    if let Some(author) = named_param(parameters, "author") {
        parts.push(escape_text(&author));
    } else if let Some(last) = named_param(parameters, "last") {
        let joined = match named_param(parameters, "first") {
            Some(first) => format!("{last}, {first}"),
            None => last,
        };
        parts.push(escape_text(&joined));
    }
    if let Some(date) = named_param(parameters, "date") {
        parts.push(format!("({})", escape_text(&date)));
    }
    if let Some(url) = named_param(parameters, "url") {
        if url.starts_with("http://") || url.starts_with("https://") {
            parts.push(format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                escape_attr(&url),
                escape_text(&url)
            ));
        } else {
            parts.push(escape_text(&url));
        }
    }
    if parts.is_empty() {
        return false;
    }
    state.ensure_paragraph_open();
    state.out.push_str(&format!(
        "<span class=\"tome-citation\">{}</span>",
        parts.join(". ")
    ));
    true
}

/// `{{convert|5|km|mi}}` → `5 km`: the first two positional parameters
/// joined with a space. Returns false with fewer than two positionals.
fn render_convert(state: &mut State<'_>, parameters: &[Parameter<'_>]) -> bool {
    let positional = positional_params(parameters);
    if positional.len() < 2 {
        return false;
    }
    state.ensure_paragraph_open();
    state.out.push_str(&format!(
        "<span class=\"tome-convert\">{} {}</span>",
        escape_text(&positional[0]),
        escape_text(&positional[1])
    ));
    true
}

enum PositionalPick {
    First,
    Last,
}

/// `{{lang|fr|Élysée}}` → `Élysée` (last positional); `{{IPA|/x/}}` → `/x/`
/// (first positional). Plain escaped text, no wrapper. Returns false when
/// the picked parameter is missing or empty.
fn render_positional_text(
    state: &mut State<'_>,
    parameters: &[Parameter<'_>],
    pick: PositionalPick,
) -> bool {
    let positional = positional_params(parameters);
    let text = match pick {
        PositionalPick::First => positional.first(),
        PositionalPick::Last => positional.last(),
    };
    match text {
        Some(text) if !text.is_empty() => {
            state.ensure_paragraph_open();
            state.out.push_str(&escape_text(text));
            true
        }
        _ => false,
    }
}

/// Plain-text value of a parameter's nodes, trimmed.
fn collect_param_text(nodes: &[Node<'_>]) -> String {
    let mut buf = String::new();
    collect_text_into(&mut buf, nodes);
    buf.trim().to_string()
}

/// Value of the named parameter `key` (case-insensitive), skipping
/// parameters whose value collects to empty text.
fn named_param(parameters: &[Parameter<'_>], key: &str) -> Option<String> {
    parameters.iter().find_map(|param| {
        let name_nodes = param.name.as_ref()?;
        if !collect_param_text(name_nodes).eq_ignore_ascii_case(key) {
            return None;
        }
        let value = collect_param_text(&param.value);
        if value.is_empty() { None } else { Some(value) }
    })
}

/// Text values of unnamed (positional) parameters, in order.
fn positional_params(parameters: &[Parameter<'_>]) -> Vec<String> {
    parameters
        .iter()
        .filter(|param| param.name.is_none())
        .map(|param| collect_param_text(&param.value))
        .collect()
}

/// `infobox_korean_name` → `Infobox korean name`.
fn prettify_template_name(raw: &str) -> String {
    let spaced = raw.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

fn split_external_link(nodes: &[Node<'_>]) -> (String, String) {
    let mut joined = String::new();
    collect_text_into(&mut joined, nodes);
    let trimmed = joined.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(idx) => (trimmed[..idx].to_string(), trimmed[idx..].to_string()),
        None => (trimmed.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{AllAvailableResolver, NoopLinkResolver};

    fn render(wikitext: &str) -> String {
        Renderer::new(Box::new(NoopLinkResolver)).render(wikitext)
    }

    fn render_with_available(wikitext: &str) -> String {
        Renderer::new(Box::new(AllAvailableResolver)).render(wikitext)
    }

    #[test]
    fn empty_input_renders_to_empty() {
        assert_eq!(render(""), "");
    }

    #[test]
    fn plain_paragraph_wraps_in_p() {
        assert_eq!(render("Hello world"), "<p>Hello world</p>");
    }

    #[test]
    fn bold_and_italic_emit_strong_and_em() {
        assert_eq!(
            render("'''photon''' is ''bright''"),
            "<p><strong>photon</strong> is <em>bright</em></p>"
        );
    }

    #[test]
    fn heading_starts_at_h2_by_default() {
        let out = render("== Section ==");
        assert!(out.contains("<h2 id=\"s-section\">"), "got: {out}");
        assert!(out.contains("Section"), "got: {out}");
        assert!(out.contains("</h2>"), "got: {out}");
    }

    #[test]
    fn heading_clamps_to_h6_at_top() {
        // Wikitext supports h1-h6; we clamp h1 up to h2 by default.
        let out = render("= TopLevel =");
        assert!(
            out.contains("<h2 id=\"s-toplevel\">TopLevel</h2>"),
            "got: {out}"
        );
    }

    #[test]
    fn heading_id_from_simple_text() {
        let out = render("== Early life ==");
        assert!(
            out.contains("<h2 id=\"s-early-life\">Early life</h2>"),
            "got: {out}"
        );
    }

    #[test]
    fn heading_id_slugs_punctuation_and_unicode_to_dashes() {
        let out = render("== Café & Bars! ==");
        assert!(out.contains("id=\"s-caf-bars\""), "got: {out}");
    }

    #[test]
    fn duplicate_headings_get_numbered_ids_in_document_order() {
        let out = render("== History ==\n\nFirst.\n\n== History ==\n\nSecond.");
        assert!(out.contains("id=\"s-history\""), "got: {out}");
        assert!(out.contains("id=\"s-history-2\""), "got: {out}");
    }

    #[test]
    fn heading_with_no_sluggable_chars_falls_back_to_section() {
        let out = render("== !!! ==");
        assert!(out.contains("id=\"s-section\""), "got: {out}");
    }

    #[test]
    fn html_special_chars_in_text_are_escaped() {
        assert_eq!(render("R&D <stuff>"), "<p>R&amp;D &lt;stuff&gt;</p>");
    }

    #[test]
    fn missing_internal_link_marked_with_class() {
        let out = render("See [[Photon]] for more.");
        assert!(
            out.contains("tome-missing"),
            "expected missing class, got: {out}"
        );
        assert!(out.contains("Photon"), "got: {out}");
    }

    #[test]
    fn available_internal_link_has_no_missing_class() {
        let out = render_with_available("See [[Photon]] for more.");
        assert!(
            !out.contains("tome-missing"),
            "expected no missing class, got: {out}"
        );
        assert!(out.contains("href=\"#/article/Photon\""), "got: {out}");
    }

    #[test]
    fn piped_link_uses_display_text() {
        let out = render_with_available("See [[Photon|the bright thing]] please.");
        assert!(out.contains("href=\"#/article/Photon\""), "got: {out}");
        assert!(out.contains("the bright thing"), "got: {out}");
    }

    #[test]
    fn external_link_renders_with_target_blank() {
        let out = render("Go to [https://example.com Example].");
        assert!(out.contains("href=\"https://example.com\""), "got: {out}");
        assert!(out.contains("target=\"_blank\""), "got: {out}");
        assert!(out.contains("rel=\"noopener noreferrer\""), "got: {out}");
        assert!(out.contains(">Example<"), "got: {out}");
    }

    #[test]
    fn external_link_without_label_uses_url() {
        let out = render("Go to [https://example.com].");
        assert!(out.contains(">https://example.com<"), "got: {out}");
    }

    #[test]
    fn unordered_list_emits_ul_li() {
        let out = render("* alpha\n* beta\n* gamma");
        assert!(out.contains("<ul>"), "got: {out}");
        assert!(out.contains("<li>alpha</li>"), "got: {out}");
        assert!(out.contains("<li>beta</li>"), "got: {out}");
        assert!(out.contains("<li>gamma</li>"), "got: {out}");
    }

    #[test]
    fn ordered_list_emits_ol_li() {
        let out = render("# first\n# second");
        assert!(out.contains("<ol>"), "got: {out}");
        assert!(out.contains("<li>first</li>"), "got: {out}");
        assert!(out.contains("<li>second</li>"), "got: {out}");
    }

    #[test]
    fn unknown_template_renders_as_placeholder() {
        let out = render("{{citation needed|date=June 2026}}");
        assert!(out.contains("tome-template-placeholder"), "got: {out}");
        assert!(out.contains("Template:"), "got: {out}");
        assert!(out.contains("citation needed"), "got: {out}");
    }

    #[test]
    fn infobox_renders_named_params_as_rows_and_skips_positional() {
        let out =
            render("{{Infobox person|positional junk|name=Ada Lovelace|occupation=Mathematician}}");
        assert!(out.contains("<div class=\"tome-infobox\">"), "got: {out}");
        assert!(
            out.contains("<div class=\"tome-infobox-title\">Infobox person</div>"),
            "got: {out}"
        );
        assert!(
            out.contains("<table class=\"tome-infobox-params\">"),
            "got: {out}"
        );
        assert!(
            out.contains("<tr><th>name</th><td>Ada Lovelace</td></tr>"),
            "got: {out}"
        );
        assert!(
            out.contains("<tr><th>occupation</th><td>Mathematician</td></tr>"),
            "got: {out}"
        );
        assert_eq!(out.matches("<tr>").count(), 2, "got: {out}");
        assert!(!out.contains("positional junk"), "got: {out}");
    }

    #[test]
    fn infobox_name_underscores_prettified_in_title() {
        let out = render("{{infobox_settlement|population=42}}");
        assert!(
            out.contains("<div class=\"tome-infobox-title\">Infobox settlement</div>"),
            "got: {out}"
        );
        assert!(
            out.contains("<tr><th>population</th><td>42</td></tr>"),
            "got: {out}"
        );
    }

    #[test]
    fn infobox_closes_open_paragraph_before_block() {
        let out = render("Intro text {{Infobox star|name=Sol}} trailing text.");
        assert!(
            out.contains("</p><div class=\"tome-infobox\">"),
            "got: {out}"
        );
    }

    #[test]
    fn infobox_with_only_positional_params_falls_back_to_placeholder() {
        let out = render("{{infobox|alpha|beta}}");
        assert!(out.contains("tome-template-placeholder"), "got: {out}");
        assert!(!out.contains("tome-infobox\""), "got: {out}");
    }

    #[test]
    fn cite_web_renders_title_and_url_link() {
        let out = render("{{cite web|title=Photon basics|url=https://example.com/photon}}");
        assert!(out.contains("<span class=\"tome-citation\">"), "got: {out}");
        assert!(out.contains("\u{201C}Photon basics\u{201D}"), "got: {out}");
        assert!(
            out.contains(
                "<a href=\"https://example.com/photon\" target=\"_blank\" rel=\"noopener noreferrer\">https://example.com/photon</a>"
            ),
            "got: {out}"
        );
        assert!(!out.contains("tome-template-placeholder"), "got: {out}");
    }

    #[test]
    fn cite_url_with_quote_is_escaped_in_attribute() {
        let out = render("{{cite web|title=X|url=https://example.com/\"q}}");
        assert!(
            out.contains("href=\"https://example.com/&quot;q\""),
            "got: {out}"
        );
        assert!(!out.contains("/\"q"), "raw quote leaked: {out}");
    }

    #[test]
    fn cite_non_http_url_renders_as_plain_text() {
        let out = render("{{cite web|title=X|url=ftp://example.com/file}}");
        assert!(out.contains("ftp://example.com/file"), "got: {out}");
        assert!(!out.contains("<a href=\"ftp:"), "got: {out}");
    }

    #[test]
    fn citation_joins_author_last_first_and_date() {
        let out = render("{{citation|title=On Photons|last=Curie|first=Marie|date=1903}}");
        assert!(
            out.contains("\u{201C}On Photons\u{201D}. Curie, Marie. (1903)"),
            "got: {out}"
        );
    }

    #[test]
    fn citation_without_recognized_params_falls_back_to_placeholder() {
        let out = render("{{citation|publisher=Acme Press}}");
        assert!(out.contains("tome-template-placeholder"), "got: {out}");
        assert!(!out.contains("tome-citation"), "got: {out}");
    }

    #[test]
    fn convert_renders_first_two_positional_params() {
        let out = render("{{convert|5|km|mi}}");
        assert!(
            out.contains("<span class=\"tome-convert\">5 km</span>"),
            "got: {out}"
        );
    }

    #[test]
    fn convert_with_one_positional_param_falls_back_to_placeholder() {
        let out = render("{{convert|5}}");
        assert!(out.contains("tome-template-placeholder"), "got: {out}");
    }

    #[test]
    fn lang_template_renders_last_positional_param() {
        let out = render("{{lang|fr|Élysée}}");
        assert!(out.contains("Élysée"), "got: {out}");
        assert!(!out.contains("tome-template-placeholder"), "got: {out}");
    }

    #[test]
    fn lang_template_without_params_falls_back_to_placeholder() {
        let out = render("{{lang}}");
        assert!(out.contains("tome-template-placeholder"), "got: {out}");
    }

    #[test]
    fn ipa_template_renders_first_positional_param() {
        let out = render("{{IPA|/fonetik/}}");
        assert!(out.contains("/fonetik/"), "got: {out}");
        assert!(!out.contains("tome-template-placeholder"), "got: {out}");
    }

    #[test]
    fn cite_template_inside_ref_formats_in_footnote() {
        let out = render("Fact.<ref>{{cite web|title=X|url=https://example.com}}</ref>");
        assert!(out.contains("<li id=\"ref-1\">"), "got: {out}");
        assert!(out.contains("tome-citation"), "got: {out}");
        assert!(out.contains("\u{201C}X\u{201D}"), "got: {out}");
        assert!(
            out.contains("<a href=\"https://example.com\""),
            "got: {out}"
        );
        assert!(!out.contains("tome-template-placeholder"), "got: {out}");
    }

    #[test]
    fn ref_tag_collected_as_footnote() {
        let out = render("Photons are real.<ref>Some source 1.</ref> Honest.<ref>Source 2.</ref>");
        assert!(out.contains("tome-ref"), "got: {out}");
        assert!(out.contains("[1]"), "got: {out}");
        assert!(out.contains("[2]"), "got: {out}");
        assert!(out.contains("tome-references"), "got: {out}");
        assert!(out.contains("Some source 1."), "got: {out}");
        assert!(out.contains("Source 2."), "got: {out}");
    }

    #[test]
    fn paragraph_break_splits_into_two_paragraphs() {
        let out = render("First paragraph.\n\nSecond paragraph.");
        // Should have at least two <p> tags
        let p_count = out.matches("<p>").count();
        assert!(p_count >= 2, "expected 2+ paragraphs, got: {out}");
    }

    #[test]
    fn unclosed_bold_is_auto_closed_at_block_boundary() {
        // '''photon (no closing) followed by paragraph break should still
        // produce balanced HTML.
        let out = render("'''photon\n\nNext paragraph.");
        // No dangling <strong> without its </strong>
        let opens = out.matches("<strong>").count();
        let closes = out.matches("</strong>").count();
        assert_eq!(opens, closes, "tags not balanced: {out}");
    }

    #[test]
    fn nowiki_tag_emits_escaped_content() {
        let out = render("This is <nowiki>'''not bold'''</nowiki> here.");
        assert!(
            out.contains("&#39;&#39;&#39;not bold&#39;&#39;&#39;"),
            "got: {out}"
        );
        assert!(!out.contains("<strong>"), "got: {out}");
    }

    #[test]
    fn horizontal_rule_renders_as_hr() {
        let out = render("Above\n----\nBelow");
        assert!(out.contains("<hr/>"), "got: {out}");
    }
}
