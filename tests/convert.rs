//! What the converter makes of each construct: prose survives byte-for-byte,
//! markup becomes the element it means, and code mode says nothing -- except
//! that a `[content]` block anywhere inside it is prose.

use typst2vast::to_vale_ast;

fn has(doc: &str, want: &[&str], absent: &[&str]) {
    let html = to_vale_ast(doc);
    for w in want {
        assert!(html.contains(w), "missing {w:?} in:\n{html}");
    }
    for a in absent {
        assert!(!html.contains(a), "unwanted {a:?} in:\n{html}");
    }
}

#[test]
fn headings() {
    has(
        "= Top\n\n== Deeper <sec-label>\n\nProse.\n",
        &["<h1>Top</h1>", "<h2>Deeper</h2>", "<p>Prose.</p>"],
        &["sec-label"],
    );
}

#[test]
fn emphasis_and_strong() {
    has(
        "See _soft words_ and *hard words* here.\n",
        &["<em>soft words</em>", "<strong>hard words</strong>"],
        &["_", "*"],
    );
}

#[test]
fn raw_inline_and_block() {
    has(
        "An `inline span` and:\n\n```python\nx = 1\n```\n",
        &["<code>inline span</code>", "<pre><code>"],
        &["python", "```"],
    );
}

#[test]
fn math_is_code() {
    has(
        "Inline $x^2$ math.\n\n$ sum_(k=1)^n k $\n",
        &["<code>x^2</code>"],
        &["$"],
    );
}

#[test]
fn lists() {
    has(
        "- One bullet.\n- Two bullet.\n\n+ First step.\n\n/ Term: Its meaning.\n",
        &["<ul>", "<li>One bullet.</li>", "<ol>", "<li>First step.</li>", "Its meaning."],
        &[],
    );
}

#[test]
fn labels_and_refs_are_markup() {
    has(
        "As @intro shows <the-label>, prose continues.\n",
        &["prose continues."],
        &["intro", "the-label"],
    );
}

#[test]
fn ref_supplement_is_prose() {
    has(
        "See @fig[the supplement text] here.\n",
        &["the supplement text"],
        &["@fig", ">fig<"],
    );
}

#[test]
fn comments_are_markup() {
    has(
        "Before.\n// line secret\n/* block /* nested */ secret */\nAfter.\n",
        &["Before.", "After."],
        &["secret"],
    );
}

#[test]
fn code_mode_is_masked() {
    has(
        "#import \"@preview/x:0.1.0\": y\n#set page(numbering: \"1\")\n\nThe total is #calc.round(1.234, digits: 2) units.\n",
        &["The total is  units."],
        &["calc", "preview", "numbering"],
    );
}

#[test]
fn content_blocks_in_code_are_prose() {
    has(
        "#figure(\n  image(\"f.png\"),\n  caption: [A caption sentence.],\n)\n\n#align(center)[Centered prose.]\n",
        &["A caption sentence.", "Centered prose."],
        &["f.png", "center)"],
    );
}

#[test]
fn let_bound_content_is_prose() {
    has(
        "#let warn(body) = [*Careful:* #body]\n",
        &["<strong>Careful:</strong>"],
        &["body"],
    );
}

#[test]
fn conditional_content_is_prose() {
    has(
        "#if release [Shipping prose.] else [Draft prose.]\n",
        &["Shipping prose.", "Draft prose."],
        &["release"],
    );
}

#[test]
fn escapes_resolve() {
    has(
        "Escaped \\*stars\\* stay literal.\n",
        &["*stars* stay literal."],
        &["\\"],
    );
}

#[test]
fn broken_documents_still_convert() {
    // The parser is error-tolerant: a half-written document still parses,
    // and the prose in and around the broken construct is kept. (An
    // unclosed *argument list* is different -- the parser reads the rest as
    // code, which is masked -- but nothing panics and nothing else is lost.)
    has(
        "= Heading\n\nProse before #strong[unclosed emphasis.\n\nProse after.\n",
        &["<h1>Heading</h1>", "Prose before", "unclosed emphasis.", "Prose after."],
        &[],
    );
}

#[test]
fn html_is_escaped() {
    has(
        "Angle <brackets> & ampersands.\n",
        &["&amp; ampersands."],
        &[],
    );
}

#[test]
fn display_math_ends_the_text_before_it() {
    let html = to_vale_ast("The bounds satisfy\n$ x <= y $\nThe next sentence.\n");
    assert!(html.contains("<p>The bounds satisfy</p>"), "{html}");
    assert!(html.contains("<pre><code>x &lt;= y</code></pre>"), "{html}");
    assert!(html.contains("<p>The next sentence.</p>"), "{html}");
}

#[test]
fn inline_math_stays_in_its_sentence() {
    has("Inline $x$ math.\n", &["<p>Inline <code>x</code> math.</p>"], &[]);
}

#[test]
fn table_cells_and_caption_are_separate_paragraphs() {
    has(
        "#figure(\n  table(columns: 2, [cell one], [cell two]),\n  caption: [A caption.],\n)\n",
        &["<p>cell one</p>", "<p>cell two</p>", "<p>A caption.</p>"],
        &["cell one  cell two"],
    );
}

#[test]
fn single_content_call_stays_inline() {
    has(
        "#link(\"https://pixi.sh\")[Pixi] installs Python.\n",
        &["<p>Pixi  installs Python.</p>"],
        &["pixi.sh"],
    );
}

#[test]
fn paragraphs_inside_a_layout_stay_apart() {
    has(
        "#grid([First para.\n\nSecond para.], [Other cell.])\n",
        &["<p>First para.</p>", "<p>Second para.</p>", "<p>Other cell.</p>"],
        &[],
    );
}
