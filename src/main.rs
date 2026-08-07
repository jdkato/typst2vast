//! typst2vast: Typst in, Vale-shaped HTML out.
//!
//! One document by default: read stdin to EOF, write the HTML. With
//! `--batch`, many documents over one process, in the length-prefixed
//! framing Vale's process pool speaks:
//!
//!   request   <byteLength> LF <bytes>
//!   response  "ok " <byteLength> LF <bytes>
//!             "err " <byteLength> LF <message>
//!
//! Lengths count bytes, so multi-byte content frames correctly. Parsing is
//! error-tolerant and never fails on malformed input, but the framing keeps
//! the `err` arm so one bad frame doesn't take the process down.

use std::io::{self, BufRead, Read, Write};

use typst2vast::to_vale_ast;

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "--version") {
        println!("typst2vast {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if std::env::args().any(|a| a == "--batch") {
        return batch();
    }

    let mut doc = String::new();
    io::stdin().read_to_string(&mut doc)?;
    io::stdout().write_all(to_vale_ast(&doc).as_bytes())
}

fn batch() -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    loop {
        let mut header = String::new();
        if input.read_line(&mut header)? == 0 {
            return Ok(()); // EOF: the caller is done with us.
        }
        let header = header.trim();
        if header.is_empty() {
            continue; // Tolerate a blank line between frames.
        }

        let want: usize = match header.parse() {
            Ok(n) => n,
            Err(_) => {
                write_frame(&mut output, "err", &format!("bad frame header: {header:?}"))?;
                continue;
            }
        };

        let mut body = vec![0u8; want];
        input.read_exact(&mut body)?;

        match String::from_utf8(body) {
            Ok(doc) => write_frame(&mut output, "ok", &to_vale_ast(&doc))?,
            Err(e) => write_frame(&mut output, "err", &e.to_string())?,
        }
    }
}

fn write_frame(out: &mut impl Write, status: &str, body: &str) -> io::Result<()> {
    write!(out, "{status} {}\n", body.len())?;
    out.write_all(body.as_bytes())?;
    out.flush()
}
