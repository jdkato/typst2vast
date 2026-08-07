# typst2vast

CLI to convert [Typst](https://typst.app/) to HTML for [Vale](https://vale.sh/), parsing without evaluating.

Documents are read with [`typst-syntax`](https://crates.io/crates/typst-syntax) — the Typst compiler's own parser, which is error-tolerant and never runs document code — and the concrete syntax tree is converted directly:

- Prose is emitted byte-for-byte from the source, so a linter can search any alert back to its position.
- Markup becomes the element it means: `=` headings, `*strong*`, `_emphasis_`, lists and term lists, raw text and math as skipped code.
- Code mode is dropped, except that a `[content]` block anywhere inside it — a figure's caption, a conditional's branches, a `#let`-bound body — is prose and is emitted in place.

Because nothing is compiled: a document that doesn't build still converts, no package is ever fetched, and no `#show` rule can make the converted text differ from the source.

## Installation

```console
$ cargo install typst2vast
```

## Usage

One document per invocation:

```console
$ typst2vast < doc.typ
```

Or many over one process with `--batch`, using length-prefixed framing:

```text
request   <byteLength> LF <bytes>
response  "ok " <byteLength> LF <bytes>
          "err " <byteLength> LF <message>
```

Lengths count bytes, so multi-byte content frames correctly. Vale uses batch mode automatically when it's available.
