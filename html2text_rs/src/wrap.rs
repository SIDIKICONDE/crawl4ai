//! Port of Python stdlib `textwrap` (the subset used by html2text's
//! `optwrap`: `wrap(text, width, break_long_words=False, subsequent_indent=...)`).

use fancy_regex::Regex;
use std::sync::LazyLock;

const PY_WS: &str = " \t\n\x0b\x0c\r";
// Python \w == [L* Nd Pc Join_Control]
const WORD: &str = "[\\p{L}\\p{Nd}\\p{Pc}\\u200c\\u200d]";
// Python letter == [^\d\W]
const LETTER: &str = "[\\p{L}\\p{Pc}\\u200c\\u200d]";

// Port of TextWrapper.wordsep_re (VERBOSE) with the same substitution.
static WORDSEP: LazyLock<Regex> = LazyLock::new(|| {
    let word = WORD.trim_matches(&['[', ']'][..]);
    let letter = LETTER.trim_matches(&['[', ']'][..]);
    let pat = format!(
        r"( [ \t\n\x0b\x0c\r]+ | (?<={wp}) -{{2,}} (?={w}) | [^ \t\n\x0b\x0c\r]+? (?: - (?: (?<={l}{{2}}-)|(?<={l}-{l}-) ) (?={l} -? {l}) | (?=[ \t\n\x0b\x0c\r]|\z) | (?<={wp}) (?=-{{2,}}{w}) ) )",
        wp = format!("[{}!\"'&.,?]", word),
        w = word,
        l = letter,
    );
    // Python compiles this pattern with re.VERBOSE (whitespace outside
    // character classes is ignored); fancy-regex has no verbose mode, so
    // strip it here.
    let pat = strip_verbose(pat);
    Regex::new(&pat).unwrap()
});

fn strip_verbose(pattern: String) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut in_class = false;
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(c);
                if let Some(&n) = chars.peek() {
                    out.push(n);
                    chars.next();
                }
            }
            '[' => {
                in_class = true;
                out.push(c);
            }
            ']' => {
                in_class = false;
                out.push(c);
            }
            ' ' | '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' if !in_class => {}
            _ => out.push(c),
        }
    }
    out
}

fn expand_tabs(text: &str, tabsize: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut col = 0usize;
    for c in text.chars() {
        if c == '\t' {
            let n = tabsize - (col % tabsize);
            for _ in 0..n {
                out.push(' ');
            }
            col += n;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// `_munge_whitespace`: expandtabs(8) then translate \t\n\v\f\r -> space.
fn munge_whitespace(text: &str) -> String {
    let expanded = expand_tabs(text, 8);
    let mut out = String::with_capacity(expanded.len());
    for c in expanded.chars() {
        if PY_WS.contains(c) {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Python `re.split(r'(\s+|...)', text)` with captures included.
pub fn split_chunks(text: &str) -> Vec<String> {
    let munged = munge_whitespace(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut last = 0usize;
    for m in WORDSEP.find_iter(&munged) {
        let Ok(m) = m else { break };
        chunks.push(munged[last..m.start()].to_string());
        chunks.push(munged[m.start()..m.end()].to_string());
        last = m.end();
    }
    chunks.push(munged[last..].to_string());
    chunks.retain(|c| !c.is_empty());
    chunks
}

/// `_handle_long_word` with break_long_words=False.
fn handle_long_word(chunks: &mut Vec<String>, cur_line: &mut Vec<String>) {
    if cur_line.is_empty() {
        let chunk = chunks.pop().unwrap();
        cur_line.push(chunk);
    }
    // else: long word goes on a line of its own on the next pass
}

/// `_wrap_chunks` with drop_whitespace=True, break_long_words=False,
/// max_lines=None (i.e. no truncation).
fn wrap_chunks(
    chunks: &mut Vec<String>,
    width: usize,
    initial_indent: &str,
    subsequent_indent: &str,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    chunks.reverse();

    while !chunks.is_empty() {
        let mut cur_line: Vec<String> = Vec::new();
        let mut cur_len = 0usize;

        let indent = if lines.is_empty() {
            initial_indent
        } else {
            subsequent_indent
        };
        // Python allows a negative width here (indent wider than width);
        // a saturating width of 0 reproduces the "nothing fits" behaviour.
        let width = width.saturating_sub(indent.chars().count());

        // First chunk on line is whitespace -- drop it, unless this is the
        // very beginning of the text.
        if !lines.is_empty() && chunks.last().is_some_and(|c| c.trim().is_empty()) {
            chunks.pop();
        }

        while let Some(c) = chunks.last() {
            let l = c.chars().count();
            if cur_len + l <= width {
                cur_len += l;
                cur_line.push(chunks.pop().unwrap());
            } else {
                break;
            }
        }

        // The current line is full, and the next chunk is too big for any line.
        if !chunks.is_empty() && chunks.last().unwrap().chars().count() > width {
            handle_long_word(chunks, &mut cur_line);
        }

        // If the last chunk on this line is all whitespace, drop it.
        if cur_line.last().is_some_and(|c| c.trim().is_empty()) {
            cur_line.pop();
        }

        if !cur_line.is_empty() {
            lines.push(format!("{}{}", indent, cur_line.concat()));
        }
    }
    lines
}

/// Port of `textwrap.wrap(text, width, break_long_words=False, subsequent_indent=indent)`.
pub fn wrap(text: &str, width: usize, subsequent_indent: &str) -> Vec<String> {
    let mut chunks = split_chunks(text);
    wrap_chunks(&mut chunks, width, "", subsequent_indent)
}
