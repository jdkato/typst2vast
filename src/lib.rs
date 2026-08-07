//! Convert Typst markup to the HTML Vale's walker reads.
//!
//! The document is parsed with `typst-syntax` -- the compiler's own parser,
//! which is error-tolerant and never evaluates anything -- and the concrete
//! syntax tree is walked directly. Prose is emitted byte-for-byte from the
//! source, so a linter can search any alert back to its position; markup
//! becomes the element it means; and code mode is dropped, except that a
//! `[content]` block anywhere inside it is prose and is emitted in place.

use typst_syntax::{parse, SyntaxKind, SyntaxNode};

/// Convert one Typst document to HTML.
pub fn to_vale_ast(doc: &str) -> String {
    let root = parse(doc);
    let mut out = Emitter::default();
    out.markup(&root);
    out.finish()
}

#[derive(Default)]
struct Emitter {
    html: String,
    para: String,
    list: Option<&'static str>,
}

impl Emitter {
    fn finish(mut self) -> String {
        self.flush();
        self.close_list();
        self.html
    }

    fn flush(&mut self) {
        let text = self.para.trim();
        if !text.is_empty() {
            self.html.push_str("<p>");
            self.html.push_str(text);
            self.html.push_str("</p>\n");
        }
        self.para.clear();
    }

    fn close_list(&mut self) {
        if let Some(tag) = self.list.take() {
            self.html.push_str("</");
            self.html.push_str(tag);
            self.html.push_str(">\n");
        }
    }

    fn open_list(&mut self, tag: &'static str) {
        if self.list != Some(tag) {
            self.close_list();
            self.html.push('<');
            self.html.push_str(tag);
            self.html.push_str(">\n");
            self.list = Some(tag);
        }
    }

    /// Walk a `Markup` node at block level.
    fn markup(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::Heading => {
                    self.flush();
                    self.close_list();
                    let level = child
                        .children()
                        .find(|c| c.kind() == SyntaxKind::HeadingMarker)
                        .map(|m| m.text().len().min(6))
                        .unwrap_or(1);
                    let body = self.collect_inline(child);
                    self.html
                        .push_str(&format!("<h{level}>{}</h{level}>\n", body.trim()));
                }
                SyntaxKind::ListItem => self.item("ul", child),
                SyntaxKind::EnumItem => self.item("ol", child),
                SyntaxKind::TermItem => self.item("ul", child),
                SyntaxKind::Parbreak => {
                    self.flush();
                    self.close_list();
                }
                SyntaxKind::Raw if is_block_raw(child) => {
                    self.flush();
                    self.close_list();
                    let body = raw_body(child);
                    self.html
                        .push_str(&format!("<pre><code>{}</code></pre>\n", escape(&body)));
                }
                _ => self.inline(child),
            }
        }
    }

    fn item(&mut self, tag: &'static str, node: &SyntaxNode) {
        self.flush();
        self.open_list(tag);
        let body = self.collect_inline(node);
        self.html
            .push_str(&format!("<li>{}</li>\n", body.trim()));
    }

    /// Emit node's inline content into a fresh buffer and hand it back.
    fn collect_inline(&mut self, node: &SyntaxNode) -> String {
        let mut inner = Emitter::default();
        for child in node.children() {
            inner.inline(child);
        }
        // A nested block construct (a list inside a term item, say) flushed
        // into inner.html; inline content remains in inner.para.
        let mut body = inner.html;
        body.push_str(&inner.para);
        body
    }

    /// Walk any node in inline position.
    fn inline(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::Text | SyntaxKind::SmartQuote | SyntaxKind::Shorthand => {
                self.para.push_str(&escape(node.text()));
            }
            SyntaxKind::Space => {
                self.para.push(' ');
            }
            SyntaxKind::Linebreak => {
                self.para.push(' ');
            }
            SyntaxKind::Parbreak => {
                self.para.push(' ');
            }
            SyntaxKind::Escape => {
                // `\*` resolves to `*`; the source form would read as markup.
                let t = node.text();
                if let Some(ch) = t.chars().last() {
                    if !t.starts_with("\\u") {
                        self.para.push_str(&escape(&ch.to_string()));
                    }
                }
            }
            SyntaxKind::Strong => self.styled(node, "strong"),
            SyntaxKind::Emph => self.styled(node, "em"),
            SyntaxKind::Raw => {
                let body = raw_body(node);
                self.para
                    .push_str(&format!("<code>{}</code>", escape(&body)));
            }
            SyntaxKind::Equation => {
                let body: String = node
                    .children()
                    .filter(|c| c.kind() != SyntaxKind::Dollar)
                    .map(text_of)
                    .collect();
                self.para
                    .push_str(&format!("<code>{}</code>", escape(body.trim())));
            }
            SyntaxKind::Link => {
                self.para
                    .push_str(&format!("<a href=\"#\">{}</a>", escape(node.text())));
            }
            SyntaxKind::Label | SyntaxKind::LineComment | SyntaxKind::BlockComment => {}
            SyntaxKind::Ref => {
                // The target says nothing; a `[supplement]` is prose.
                for child in node.children() {
                    if child.kind() == SyntaxKind::ContentBlock {
                        self.code(child);
                    }
                }
            }
            SyntaxKind::Markup => {
                for child in node.children() {
                    self.inline(child);
                }
            }
            _ => self.code(node),
        }
    }

    /// Walk a code-mode subtree: everything is dropped except the `Markup`
    /// inside a content block, which is prose in disguise.
    fn code(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::Markup => {
                self.para.push(' ');
                for child in node.children() {
                    self.inline(child);
                }
                self.para.push(' ');
            }
            _ => {
                for child in node.children() {
                    self.code(child);
                }
            }
        }
    }

    fn styled(&mut self, node: &SyntaxNode, tag: &str) {
        let mut inner = Emitter::default();
        for child in node.children() {
            if !matches!(child.kind(), SyntaxKind::Star | SyntaxKind::Underscore) {
                inner.inline(child);
            }
        }
        let body = inner.para;
        self.para
            .push_str(&format!("<{tag}>{}</{tag}>", body.trim()));
    }
}

/// The source text of a whole subtree.
fn text_of(node: &SyntaxNode) -> String {
    if node.children().len() == 0 {
        return node.text().to_string();
    }
    node.children().map(text_of).collect()
}

fn is_block_raw(node: &SyntaxNode) -> bool {
    node.children()
        .find(|c| c.kind() == SyntaxKind::RawDelim)
        .map(|d| d.text().len() >= 3)
        .unwrap_or(false)
}

/// A raw region's text, delimiters and language tag dropped.
fn raw_body(node: &SyntaxNode) -> String {
    node.children()
        .filter(|c| !matches!(c.kind(), SyntaxKind::RawDelim | SyntaxKind::RawLang))
        .map(text_of)
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
