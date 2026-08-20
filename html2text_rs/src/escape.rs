//! Markdown escaping helpers mirroring crawl4ai/html2text/utils.py (escape parts)
//!
//! Regexes are ported verbatim from the Python module. `\s`, `\w`, `\d` keep
//! Python `re` semantics thanks to fancy-regex Unicode support.

use fancy_regex::Regex;
use std::sync::LazyLock;

// RE_MD_CHARS_MATCHER = re.compile(r"([\\\[\]\(\)])")
static RE_MD_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([\\\[\]\(\)])").unwrap());

// RE_MD_CHARS_MATCHER_ALL = re.compile(r"([`\*_{}\[\]\(\)#!])")
static RE_MD_CHARS_ALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([`\*_{}\[\]\(\)#!])").unwrap());

// RE_MD_BACKSLASH_MATCHER: (\\)(?=[`\*_{}[]()#+\-.!])
static RE_MD_BACKSLASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\\)(?=[`\*_{}\[\]\(\)#+\-.!])").unwrap());

// RE_MD_DOT_MATCHER (MULTILINE): ^(\s*\d+)(\.)(?=\s)
static RE_MD_DOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(\s*\d+)(\.)(?=\s)").unwrap());

// RE_MD_PLUS_MATCHER (MULTILINE): ^(\s*)(\+)(?=\s)
static RE_MD_PLUS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(\s*)(\+)(?=\s)").unwrap());

// RE_MD_DASH_MATCHER (MULTILINE): ^(\s*)(-)(?=\s|\-)
static RE_MD_DASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(\s*)(-)(?=\s|\-)").unwrap());

// RE_SPACE = re.compile(r"\s\+")
static RE_SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s\+").unwrap());

// RE_LINK = (\[.*?\] ?\(.*?\))|(\[.*?\]:.*?)
static RE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\[.*?\] ?\(.*?\))|(\[.*?\]:.*?)").unwrap());

// RE_TABLE = re.compile(r" \| ")
static RE_TABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" \| ").unwrap());

// RE_ORDERED_LIST_MATCHER = \d+\.\s
static RE_ORDERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+\.\s").unwrap());

// RE_UNORDERED_LIST_MATCHER = [-\*\+]\s
static RE_UNORDERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[-\*\+]\s").unwrap());

pub fn re_space_matches(para: &str) -> bool {
    RE_SPACE
        .find_from_pos(para, 0)
        .map(|m| m.map(|m| m.start() == 0).unwrap_or(false))
        .unwrap_or(false)
}

pub fn re_link_search(para: &str) -> bool {
    RE_LINK.find(para).map(|m| m.is_some()).unwrap_or(false)
}

pub fn re_table_search(para: &str) -> bool {
    RE_TABLE.find(para).map(|m| m.is_some()).unwrap_or(false)
}

pub fn re_ordered_match(para: &str) -> bool {
    RE_ORDERED
        .find_from_pos(para, 0)
        .map(|m| m.map(|m| m.start() == 0).unwrap_or(false))
        .unwrap_or(false)
}

pub fn re_unordered_match(para: &str) -> bool {
    RE_UNORDERED
        .find_from_pos(para, 0)
        .map(|m| m.map(|m| m.start() == 0).unwrap_or(false))
        .unwrap_or(false)
}

/// Escapes markdown-sensitive characters within other markdown constructs.
pub fn escape_md(text: &str) -> String {
    RE_MD_CHARS.replace_all(text, r"\\$1").into_owned()
}

/// Escapes markdown-sensitive characters across whole document sections.
/// Each escaping operation can be controlled individually.
pub fn escape_md_section(
    text: &str,
    escape_backslash: bool,
    snob: bool,
    escape_dot: bool,
    escape_plus: bool,
    escape_dash: bool,
) -> String {
    let mut out = text.to_string();
    if escape_backslash {
        out = RE_MD_BACKSLASH.replace_all(&out, r"\\$1").into_owned();
    }
    if snob {
        out = RE_MD_CHARS_ALL.replace_all(&out, r"\\$1").into_owned();
    }
    if escape_dot {
        out = RE_MD_DOT.replace_all(&out, r"$1\\$2").into_owned();
    }
    if escape_plus {
        out = RE_MD_PLUS.replace_all(&out, r"$1\\$2").into_owned();
    }
    if escape_dash {
        out = RE_MD_DASH.replace_all(&out, r"$1\\$2").into_owned();
    }
    out
}

/// Decide whether a paragraph must not be wrapped (mirror of utils.skipwrap).
pub fn skipwrap(para: &str, wrap_links: bool, wrap_list_items: bool, wrap_tables: bool) -> bool {
    if !wrap_links && re_link_search(para) {
        return true;
    }
    if para.len() >= 4 && para.as_bytes().starts_with(b"    ") {
        return true;
    }
    if para.starts_with('\t') {
        return true;
    }

    let stripped = para.trim_start();
    if stripped.len() >= 2
        && stripped.as_bytes().starts_with(b"--")
        && stripped.len() > 2
        && stripped.as_bytes()[2] != b'-'
    {
        return false;
    }

    let first = stripped.chars().next();
    if matches!(first, Some('-') | Some('*')) && !stripped.as_bytes().starts_with(b"**") {
        return !wrap_list_items;
    }

    if !wrap_tables && re_table_search(para) {
        return true;
    }

    re_ordered_match(stripped) || re_unordered_match(stripped)
}
