//! Convert Typst markup to the HTML Vale's walker reads.
//!
//! The document is parsed with `typst-syntax` -- the compiler's own parser,
//! which is error-tolerant and never evaluates anything -- and the concrete
//! syntax tree is walked directly. Prose is emitted byte-for-byte from the
//! source, so a linter can search any alert back to its position; markup
//! becomes the element it means; and code mode is dropped, except that a
//! `[content]` block anywhere inside it is prose and is emitted in place.
//!
//! A handful of the standard library's functions are markup by another
//! name, and are read by their names alone -- nothing is evaluated, so a
//! document that rebinds `table` is read as if it hadn't: `link`, `image`,
//! `figure`, `table`, `quote`, `line`, `emph` and `strong` become the
//! elements they mean, with what they point at (a link's target, an
//! image's path) in attributes, where a linter's walker doesn't read prose.
//! A label is its element's `id`.

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
        let children: Vec<&SyntaxNode> = node.children().collect();
        let mut skip = None;
        for (i, child) in children.iter().copied().enumerate() {
            if skip == Some(i) {
                continue;
            }
            match child.kind() {
                SyntaxKind::Heading => {
                    self.flush();
                    self.close_list();
                    let level = child
                        .children()
                        .find(|c| c.kind() == SyntaxKind::HeadingMarker)
                        .map(|m| m.text().len().min(6))
                        .unwrap_or(1);
                    // `== Title <label>`: the label is the heading's id.
                    let label = children[i + 1..]
                        .iter()
                        .enumerate()
                        .take_while(|(_, c)| matches!(c.kind(), SyntaxKind::Space | SyntaxKind::Label))
                        .find(|(_, c)| c.kind() == SyntaxKind::Label)
                        .map(|(k, c)| (i + 1 + k, label_name(c)));
                    let id = match &label {
                        Some((at, name)) => {
                            skip = Some(*at);
                            format!(" id=\"{}\"", attr(name))
                        }
                        None => String::new(),
                    };
                    let body = self.collect_inline(child);
                    self.html
                        .push_str(&format!("<h{level}{id}>{}</h{level}>\n", body.trim()));
                }
                SyntaxKind::Equation if is_block_equation(child) => {
                    self.flush();
                    self.close_list();
                    self.html.push_str(&format!(
                        "<math display=\"block\"><code>{}</code></math>\n",
                        escape(equation_body(child).trim())
                    ));
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
                self.para.push_str(&format!(
                    "<code>{}</code>",
                    escape(equation_body(node).trim())
                ));
            }
            SyntaxKind::Link => {
                // A bare URL is its own target.
                self.para.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    attr(node.text()),
                    escape(node.text())
                ));
            }
            SyntaxKind::Label => {
                // A label on anything but a heading: a place to point at.
                self.para
                    .push_str(&format!("<a id=\"{}\"></a>", attr(&label_name(node))));
            }
            SyntaxKind::FuncCall => self.call(node),
            SyntaxKind::LineComment | SyntaxKind::BlockComment => {}
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

    /// A function call: the standard library's markup functions become
    /// their elements; any other is code, its content blocks prose.
    fn call(&mut self, node: &SyntaxNode) {
        let name = callee(node);
        let Some(args) = node.children().find(|c| c.kind() == SyntaxKind::Args) else {
            return self.code(node);
        };
        match name.as_str() {
            "link" => {
                let Some(href) = first_target(args) else {
                    return self.code(node);
                };
                let body = self.content_of(args);
                self.para
                    .push_str(&format!("<a href=\"{}\">{}</a>", attr(&href), body.trim()));
            }
            "image" => match image(args) {
                Some(img) => self.para.push_str(&img),
                None => self.code(node),
            },
            "figure" => {
                let mut inner = Emitter::default();
                let mut caption = String::new();
                for a in args.children() {
                    match a.kind() {
                        SyntaxKind::Named if named_key(a) == "caption" => {
                            let mut c = Emitter::default();
                            c.code(a);
                            caption = c.into_inline();
                        }
                        SyntaxKind::Named => {}
                        _ => inner.arg(a),
                    }
                }
                let body = inner.into_inline();
                self.flush();
                self.html.push_str("<figure>");
                self.html.push_str(body.trim());
                if !caption.trim().is_empty() {
                    self.html
                        .push_str(&format!("<figcaption>{}</figcaption>", caption.trim()));
                }
                self.html.push_str("</figure>\n");
            }
            "table" => self.table(args),
            "quote" => {
                let block = args.children().any(|a| {
                    a.kind() == SyntaxKind::Named
                        && named_key(a) == "block"
                        && a.children().any(|v| v.kind() == SyntaxKind::Bool && v.text() == "true")
                });
                let body = self.content_of(args);
                if block {
                    self.flush();
                    self.html
                        .push_str(&format!("<blockquote><p>{}</p></blockquote>\n", body.trim()));
                } else {
                    self.para.push_str(&format!("<q>{}</q>", body.trim()));
                }
            }
            "line" => {
                self.flush();
                self.html.push_str("<hr>\n");
            }
            "emph" | "strong" => {
                let tag = if name == "emph" { "em" } else { "strong" };
                let body = self.content_of(args);
                self.para
                    .push_str(&format!("<{tag}>{}</{tag}>", body.trim()));
            }
            _ => self.code(node),
        }
    }

    /// An argument in a call that holds content: a nested call is read as
    /// one, anything else as code.
    fn arg(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::FuncCall => self.call(node),
            _ => self.code(node),
        }
    }

    /// The prose in a call's content blocks, as inline HTML.
    fn content_of(&mut self, args: &SyntaxNode) -> String {
        let mut inner = Emitter::default();
        for a in args.children() {
            if a.kind() == SyntaxKind::ContentBlock {
                inner.code(a);
            }
        }
        inner.into_inline()
    }

    fn into_inline(self) -> String {
        let mut body = self.html;
        body.push_str(&self.para);
        body
    }

    /// `table(columns: n, ..cells)`: rows of n cells; `table.header`'s
    /// cells are a row of headers.
    fn table(&mut self, args: &SyntaxNode) {
        let columns = args
            .children()
            .find(|a| a.kind() == SyntaxKind::Named && named_key(a) == "columns")
            .map(column_count)
            .unwrap_or(1)
            .max(1);
        let mut head: Vec<String> = Vec::new();
        let mut cells: Vec<String> = Vec::new();
        for a in args.children() {
            match a.kind() {
                SyntaxKind::ContentBlock => cells.push(cell(a)),
                SyntaxKind::FuncCall => {
                    let name = callee(a);
                    let Some(inner) = a.children().find(|c| c.kind() == SyntaxKind::Args) else {
                        continue;
                    };
                    match name.as_str() {
                        "table.header" => head.extend(
                            inner
                                .children()
                                .filter(|c| c.kind() == SyntaxKind::ContentBlock)
                                .map(cell),
                        ),
                        "table.hline" | "table.vline" => {}
                        // `table.cell(colspan: 2)[..]`, or any call that
                        // makes a cell's content.
                        _ => {
                            let mut e = Emitter::default();
                            e.arg(a);
                            cells.push(e.into_inline().trim().to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        self.flush();
        self.html.push_str("<table>\n");
        let row = |tag: &str, cells: &[String], html: &mut String| {
            html.push_str("<tr>");
            for c in cells {
                html.push_str(&format!("<{tag}>{c}</{tag}>"));
            }
            html.push_str("</tr>\n");
        };
        for r in head.chunks(columns) {
            row("th", r, &mut self.html);
        }
        for r in cells.chunks(columns) {
            row("td", r, &mut self.html);
        }
        self.html.push_str("</table>\n");
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

/// A cell's content, as inline HTML.
fn cell(block: &SyntaxNode) -> String {
    let mut e = Emitter::default();
    e.code(block);
    e.into_inline().trim().to_string()
}

/// The function a call names: `table`, or `table.header`.
fn callee(call: &SyntaxNode) -> String {
    match call.children().next() {
        Some(c) if matches!(c.kind(), SyntaxKind::Ident | SyntaxKind::FieldAccess) => text_of(c),
        _ => String::new(),
    }
}

fn named_key(named: &SyntaxNode) -> String {
    named
        .children()
        .find(|c| c.kind() == SyntaxKind::Ident)
        .map(|c| c.text().to_string())
        .unwrap_or_default()
}

/// A string literal's value.
fn string(node: &SyntaxNode) -> String {
    let t = node.text();
    let inner = t.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(t);
    inner.replace("\\\"", "\"").replace("\\\\", "\\")
}

/// `<name>` without its angle brackets.
fn label_name(node: &SyntaxNode) -> String {
    node.text().trim_start_matches('<').trim_end_matches('>').to_string()
}

/// A link's target: its first argument, a string or a label.
fn first_target(args: &SyntaxNode) -> Option<String> {
    args.children().find_map(|a| match a.kind() {
        SyntaxKind::Str => Some(string(a)),
        SyntaxKind::Label => Some(format!("#{}", label_name(a))),
        _ => None,
    })
}

/// `image("path", alt: "..")` as an `img`.
fn image(args: &SyntaxNode) -> Option<String> {
    let src = args.children().find(|a| a.kind() == SyntaxKind::Str).map(string)?;
    let alt = args
        .children()
        .find(|a| a.kind() == SyntaxKind::Named && named_key(a) == "alt")
        .and_then(|a| a.children().find(|v| v.kind() == SyntaxKind::Str).map(string))
        .unwrap_or_default();
    Some(format!("<img src=\"{}\" alt=\"{}\">", attr(&src), attr(&alt)))
}

/// `columns: 3`, or `columns: (1fr, 2fr, auto)`.
fn column_count(named: &SyntaxNode) -> usize {
    for v in named.children() {
        match v.kind() {
            SyntaxKind::Int => return v.text().parse().unwrap_or(1),
            SyntaxKind::Array => {
                return v
                    .children()
                    .filter(|c| {
                        !matches!(
                            c.kind(),
                            SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Comma | SyntaxKind::Space
                        )
                    })
                    .count()
            }
            _ => {}
        }
    }
    1
}

/// `$ x $`, with space inside its dollars, is set on its own line.
fn is_block_equation(node: &SyntaxNode) -> bool {
    let kids: Vec<&SyntaxNode> = node.children().collect();
    kids.len() >= 4
        && kids[1].kind() == SyntaxKind::Space
        && kids[kids.len() - 2].kind() == SyntaxKind::Space
}

fn equation_body(node: &SyntaxNode) -> String {
    node.children()
        .filter(|c| c.kind() != SyntaxKind::Dollar)
        .map(text_of)
        .collect()
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

fn attr(s: &str) -> String {
    escape(s).replace('"', "&quot;")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
