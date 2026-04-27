//! Wikitext-AST to HTML walker.
//!
//! Bold and italic are toggle nodes in the AST (no children) — we track state
//! across siblings and emit balanced open/close tags. Paragraphs are inferred
//! from the layout: inline content auto-opens a `<p>`, block elements
//! (headings, lists, horizontal rules) close the current paragraph,
//! `ParagraphBreak` does the same.

use parse_wiki_text_2::{Configuration, Node};

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
            resolver,
            options,
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
        self.out
            .push_str("<section class=\"tome-references\"><h2>References</h2><ol>");
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
            state.out.push_str(&format!("<h{lvl}>"));
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
                escape_attr(&resolved_target),
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
        Node::Template { name, .. } => {
            state.ensure_paragraph_open();
            let mut name_text = String::new();
            collect_text_into(&mut name_text, name);
            state.out.push_str(&format!(
                "<span class=\"tome-template-placeholder\" title=\"template not rendered locally\">[Template: {}]</span>",
                escape_text(name_text.trim())
            ));
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
                let mut buf = String::new();
                collect_text_into(&mut buf, children);
                state.refs.push(buf);
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
        assert!(out.contains("<h2>"), "got: {out}");
        assert!(out.contains("Section"), "got: {out}");
        assert!(out.contains("</h2>"), "got: {out}");
    }

    #[test]
    fn heading_clamps_to_h6_at_top() {
        // Wikitext supports h1-h6; we clamp h1 up to h2 by default.
        let out = render("= TopLevel =");
        assert!(out.contains("<h2>TopLevel</h2>"), "got: {out}");
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
    fn template_renders_as_placeholder() {
        let out = render("{{infobox|name=Photon|kind=particle}}");
        assert!(out.contains("tome-template-placeholder"), "got: {out}");
        assert!(out.contains("Template:"), "got: {out}");
        assert!(out.contains("infobox"), "got: {out}");
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
