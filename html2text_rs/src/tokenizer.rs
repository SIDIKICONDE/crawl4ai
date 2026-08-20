//! Port of CPython `Lib/html/parser.py` (HTMLParser) as used by html2text:
//! `HTMLParser(convert_charrefs=False)`.
//!
//! The tokenizer mirrors `goahead()` and the `parse_*` helpers exactly,
//! including RAWTEXT/RCDATA modes and the EOF (`end=1`) handling in `close()`.

use fancy_regex::Regex;
use std::sync::LazyLock;

pub const CDATA_CONTENT_ELEMENTS: [&str; 7] = [
    "script",
    "style",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
];
pub const RCDATA_CONTENT_ELEMENTS: [&str; 2] = ["textarea", "title"];

#[derive(Debug, Clone)]
pub enum Event {
    Data(String),
    StartTag(String, Vec<(String, Option<String>)>),
    EndTag(String),
    CharRef(String),
    EntityRef(String),
    StartEndTag(String, Vec<(String, Option<String>)>),
    Comment(String),
    Pi(String),
    Decl(String),
    Cdata(String),
    UnknownDecl(String),
}

// -- regexes with lookbehind (port of the Python regexes, same semantics) --

// locatetagend (VERBOSE): complete-start-tag check, anchored by the caller
// (Python matches it at i+1 / i+2 with .match(); we check m.start() == anchor).
static LOCATETAGEND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"[a-zA-Z][^\t\n\r\f />]*[\t\n\r\f /]*(?:(?<=['"\t\n\r\f /])[^\t\n\r\f />][^\t\n\r\f /=>]*(?:[\t\n\r\f ]*=[\t\n\r\f ]*(?:'[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\f ]*))?[\t\n\r\f /]*)*>?"#,
    )
    .unwrap()
});

// attrfind_tolerant (VERBOSE)
static ATTRFIND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"((?<=['"\t\n\r\f /])[^\t\n\r\f />][^\t\n\r\f /=>]*)([\t\n\r\f ]*=[\t\n\r\f ]*('[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\f ]*))?(?:[\t\n\r\f ]|/(?!>))*"#,
    )
    .unwrap()
});

/// Tokenizer state (mirrors HTMLParser.reset/feed/close/set_cdata_mode).
pub struct Tokenizer {
    pub rawdata: String,
    cdata_elem: Option<String>,
    escapable: bool,
}

impl Default for Tokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer {
    pub fn new() -> Self {
        Tokenizer {
            rawdata: String::new(),
            cdata_elem: None,
            escapable: false,
        }
    }

    pub fn reset(&mut self) {
        self.rawdata.clear();
        self.cdata_elem = None;
        self.escapable = false;
    }

    fn set_cdata_mode(&mut self, elem: &str, escapable: bool) {
        self.cdata_elem = Some(elem.to_string());
        self.escapable = escapable;
    }

    fn clear_cdata_mode(&mut self) {
        self.cdata_elem = None;
        self.escapable = false;
    }

    /// Position of the next interesting character (`&` or `<`, or the cdata
    /// end tag), mirroring `self.interesting.search(rawdata, i)`.
    fn interesting_from(&self, i: usize) -> Option<usize> {
        let raw = self.rawdata.as_bytes();
        if let Some(elem) = &self.cdata_elem {
            if elem == "plaintext" {
                // interesting == r'\z' -> never matches
                return None;
            }
            // interesting == r'</%s(?=[\t\n\r\f />])' (IGNORECASE|ASCII),
            // plus '&' when escapable (convert_charrefs=False path)
            let needle = format!("</{}", elem.to_ascii_lowercase());
            let mut found: Option<usize> = None;
            let mut p = i;
            while p + needle.len() <= raw.len() {
                let mut ok = true;
                for (k, &nb) in needle.as_bytes().iter().enumerate() {
                    if raw[p + k].to_ascii_lowercase() != nb {
                        ok = false;
                        break;
                    }
                }
                if ok
                    && raw
                        .get(p + needle.len())
                        .is_some_and(|c| b" \t\n\r\x0C/>".contains(c))
                {
                    found = Some(p);
                    break;
                }
                p += 1;
            }
            if self.escapable {
                let amp = self.rawdata[i..].find('&').map(|p| i + p);
                match (found, amp) {
                    (Some(f), Some(a)) => Some(f.min(a)),
                    (Some(f), None) => Some(f),
                    (None, Some(a)) => Some(a),
                    (None, None) => None,
                }
            } else {
                found
            }
        } else {
            let amp = self.rawdata[i..].find('&').map(|p| i + p);
            let lt = self.rawdata[i..].find('<').map(|p| i + p);
            match (amp, lt) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }
        }
    }

    pub fn feed(&mut self, data: &str, events: &mut Vec<Event>) {
        self.rawdata.push_str(data);
        self.goahead(false, events);
    }

    pub fn close(&mut self, events: &mut Vec<Event>) {
        self.goahead(true, events);
    }

    fn goahead(&mut self, end: bool, events: &mut Vec<Event>) {
        let mut i = 0usize;
        let n = self.rawdata.len();
        while i < n {
            let j = match self.interesting_from(i) {
                Some(p) => p,
                None => {
                    if self.cdata_elem.is_some() {
                        break;
                    }
                    n
                }
            };
            if i < j {
                events.push(Event::Data(self.rawdata[i..j].to_string()));
            }
            i = j;
            if i == n {
                break;
            }
            let raw = self.rawdata.as_bytes();
            if raw[i] == b'<' {
                let mut k: isize;
                if is_starttagopen(&self.rawdata, i) {
                    k = self.parse_starttag(i, events);
                } else if self.rawdata[i..].starts_with("</") {
                    k = self.parse_endtag(i, events);
                } else if self.rawdata[i..].starts_with("<!--") {
                    k = self.parse_comment(i, events, true);
                } else if self.rawdata[i..].starts_with("<?") {
                    k = self.parse_pi(i, events);
                } else if self.rawdata[i..].starts_with("<!") {
                    k = self.parse_html_declaration(i, events);
                } else if i + 1 < n || end {
                    events.push(Event::Data("<".to_string()));
                    k = (i + 1) as isize;
                } else {
                    break;
                }
                if k < 0 {
                    if !end {
                        break;
                    }
                    if is_starttagopen(&self.rawdata, i) {
                        // < + letter, incomplete at EOF: dropped
                    } else if self.rawdata[i..].starts_with("</") {
                        if i + 2 == n {
                            events.push(Event::Data("</".to_string()));
                        } else if is_endtagopen(&self.rawdata, i) {
                            // </ + letter, incomplete: dropped
                        } else {
                            // bogus comment
                            events.push(Event::Comment(self.rawdata[i + 2..].to_string()));
                        }
                    } else if self.rawdata[i..].starts_with("<!--") {
                        let mut j = n;
                        for suffix in ["--!", "--", "-"] {
                            if self.rawdata.ends_with(suffix) {
                                j -= suffix.len();
                                break;
                            }
                        }
                        events.push(Event::Comment(self.rawdata[i + 4..j].to_string()));
                    } else if self.rawdata[i..].starts_with("<![CDATA[") {
                        events.push(Event::UnknownDecl(self.rawdata[i + 3..].to_string()));
                    } else if self
                        .rawdata
                        .get(i..i + 9)
                        .map(|s| s.eq_ignore_ascii_case("<!doctype"))
                        .unwrap_or(false)
                    {
                        events.push(Event::Decl(self.rawdata[i + 2..].to_string()));
                    } else if self.rawdata[i..].starts_with("<!") {
                        // bogus comment
                        events.push(Event::Comment(self.rawdata[i + 2..].to_string()));
                    } else if self.rawdata[i..].starts_with("<?") {
                        events.push(Event::Pi(self.rawdata[i + 2..].to_string()));
                    } else {
                        unreachable!("we should not get here")
                    }
                    k = n as isize;
                }
                i = k as usize;
            } else if self.rawdata[i..].starts_with("&#") {
                if let Some((k, name)) = charref_match(&self.rawdata, i) {
                    events.push(Event::CharRef(name));
                    let mut k = k;
                    if !self
                        .rawdata
                        .as_bytes()
                        .get(k - 1)
                        .is_some_and(|&c| c == b';')
                    {
                        k -= 1;
                    }
                    i = k;
                    continue;
                }
                if incomplete_charref_match(&self.rawdata, i) {
                    if end {
                        events.push(Event::CharRef(self.rawdata[i + 2..].to_string()));
                        i = n;
                        break;
                    }
                    break; // incomplete
                } else if i + 3 < n {
                    // larger than "&#x"
                    events.push(Event::Data("&#".to_string()));
                    i += 2;
                } else {
                    break;
                }
            } else if raw[i] == b'&' {
                if let Some((k, name)) = entityref_match(&self.rawdata, i) {
                    events.push(Event::EntityRef(name));
                    let mut k = k;
                    if !self
                        .rawdata
                        .as_bytes()
                        .get(k - 1)
                        .is_some_and(|&c| c == b';')
                    {
                        k -= 1;
                    }
                    i = k;
                    continue;
                }
                if incomplete_match(&self.rawdata, i) {
                    if end {
                        events.push(Event::EntityRef(self.rawdata[i + 1..].to_string()));
                        i = n;
                        break;
                    }
                    break; // incomplete
                } else if i + 1 < n {
                    events.push(Event::Data("&".to_string()));
                    i += 1;
                } else {
                    break;
                }
            } else {
                unreachable!("interesting.search() lied")
            }
        }
        if end && i < n {
            events.push(Event::Data(self.rawdata[i..n].to_string()));
            i = n;
        }
        self.rawdata.drain(..i);
    }

    // -- construct parsers (return end index, or -1 if incomplete) --

    fn parse_starttag(&mut self, i: usize, events: &mut Vec<Event>) -> isize {
        // check_for_whole_start_tag: locatetagend.match(rawdata, i+1)
        let endpos = match whole_start_tag_end(&self.rawdata, i + 1) {
            Some(e) => e,
            None => return -1,
        };

        let raw = &self.rawdata;
        let (tag, mut k) = tagfind_tolerant(raw, i + 1);
        let tag = tag.to_ascii_lowercase();

        let mut attrs: Vec<(String, Option<String>)> = Vec::new();
        while k < endpos {
            let Ok(Some(m)) = ATTRFIND.captures_from_pos(raw, k) else {
                break;
            };
            if m.get(0).is_some_and(|g| g.start() != k) {
                break;
            }
            let attrname = m.get(1).map(|g| g.as_str().to_string());
            let rest = m.get(2).map(|g| g.as_str().to_string());
            let attrvalue = m.get(3).map(|g| g.as_str().to_string());
            let (name, value) = match (attrname, rest) {
                (Some(name), Some(rest)) => {
                    let value: Option<String> = if rest.is_empty() {
                        None
                    } else {
                        let raw_v = attrvalue.unwrap_or_default();
                        let v = if raw_v.starts_with('\'') && raw_v.ends_with('\'')
                            || raw_v.starts_with('"') && raw_v.ends_with('"')
                        {
                            &raw_v[1..raw_v.len().saturating_sub(1)]
                        } else {
                            raw_v.as_str()
                        };
                        if !v.is_empty() {
                            Some(unescape_attrvalue(v))
                        } else {
                            Some(String::new())
                        }
                    };
                    (name.to_ascii_lowercase(), value)
                }
                (Some(name), None) => (name.to_ascii_lowercase(), None),
                _ => break,
            };
            attrs.push((name, value));
            k = m.get(0).map(|g| g.end()).unwrap_or(k);
        }

        let end = raw[k..endpos].trim();
        if end != ">" && end != "/>" {
            events.push(Event::Data(raw[i..endpos].to_string()));
            return endpos as isize;
        }
        if end.ends_with("/>") {
            events.push(Event::StartEndTag(tag, attrs));
        } else {
            events.push(Event::StartTag(tag.clone(), attrs));
            if CDATA_CONTENT_ELEMENTS.contains(&tag.as_str()) {
                self.set_cdata_mode(&tag, false);
            } else if RCDATA_CONTENT_ELEMENTS.contains(&tag.as_str()) {
                self.set_cdata_mode(&tag, true);
            }
        }
        endpos as isize
    }

    fn parse_endtag(&mut self, i: usize, events: &mut Vec<Event>) -> isize {
        let raw = &self.rawdata;
        if raw[i + 2..].find('>').is_none() {
            // fast check
            return -1;
        }
        if !is_endtagopen(raw, i) {
            // </ + letter failed
            if raw.as_bytes().get(i + 2) == Some(&b'>') {
                // </> is ignored
                return (i + 3) as isize;
            }
            return self.parse_bogus_comment(i, events);
        }

        let j = match whole_start_tag_end(raw, i + 2) {
            Some(j) => j,
            None => return -1,
        };
        let (tag, _) = tagfind_tolerant(raw, i + 2);
        events.push(Event::EndTag(tag.to_ascii_lowercase()));
        self.clear_cdata_mode();
        j as isize
    }

    fn parse_comment(&mut self, i: usize, events: &mut Vec<Event>, report: bool) -> isize {
        let raw = &self.rawdata;
        // commentclose: --!?>  (search from i+4); comment data ends at the '--'
        let mut j: Option<usize> = None;
        let mut p = i + 4;
        while p < raw.len() {
            let Some(rel) = raw[p..].find("--") else {
                break;
            };
            let f = p + rel;
            let after = f + 2;
            match raw.as_bytes().get(after) {
                Some(b'>') => {
                    j = Some(f);
                    break;
                }
                Some(b'!') if raw.as_bytes().get(after + 1) == Some(&b'>') => {
                    j = Some(f);
                    break;
                }
                _ => {}
            }
            p = f + 2;
        }
        let (j, end) = match j {
            Some(j) => {
                let after = j + 2;
                let end = if raw.as_bytes().get(after) == Some(&b'>') {
                    after + 1
                } else {
                    after + 2
                };
                (j, end)
            }
            None => {
                // commentabruptclose: -?>  (anchored at i+4)
                let b = raw.as_bytes();
                if b.get(i + 4) == Some(&b'>') {
                    (i + 4, i + 5)
                } else if b.get(i + 4) == Some(&b'-') && b.get(i + 5) == Some(&b'>') {
                    (i + 4, i + 6)
                } else {
                    return -1;
                }
            }
        };
        if report {
            events.push(Event::Comment(raw[i + 4..j].to_string()));
        }
        end as isize
    }

    fn parse_bogus_comment(&mut self, i: usize, events: &mut Vec<Event>) -> isize {
        let raw = &self.rawdata;
        let Some(rel) = raw[i + 2..].find('>') else {
            return -1;
        };
        let pos = i + 2 + rel;
        events.push(Event::Comment(raw[i + 2..pos].to_string()));
        (pos + 1) as isize
    }

    fn parse_pi(&mut self, i: usize, events: &mut Vec<Event>) -> isize {
        let raw = &self.rawdata;
        let Some(rel) = raw[i + 2..].find('>') else {
            return -1;
        };
        let j = i + 2 + rel;
        events.push(Event::Pi(raw[i + 2..j].to_string()));
        (j + 1) as isize
    }

    fn parse_html_declaration(&mut self, i: usize, events: &mut Vec<Event>) -> isize {
        let raw = &self.rawdata;
        if raw[i..].starts_with("<!--") {
            return self.parse_comment(i, events, true);
        }
        if raw[i..].starts_with("<![CDATA[") {
            let Some(rel) = raw[i + 9..].find("]]>") else {
                return -1;
            };
            let j = i + 9 + rel;
            events.push(Event::UnknownDecl(raw[i + 3..j].to_string()));
            return (j + 3) as isize;
        }
        if raw
            .get(i..i + 9)
            .map(|s| s.eq_ignore_ascii_case("<!doctype"))
            .unwrap_or(false)
        {
            let Some(rel) = raw[i + 9..].find('>') else {
                return -1;
            };
            let gtpos = i + 9 + rel;
            events.push(Event::Decl(raw[i + 2..gtpos].to_string()));
            return (gtpos + 1) as isize;
        }
        self.parse_bogus_comment(i, events)
    }
}

// -- low level matchers (hand-rolled ports of the Python regexes) --

fn is_starttagopen(s: &str, i: usize) -> bool {
    let b = s.as_bytes();
    b.get(i) == Some(&b'<') && b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic())
}

fn is_endtagopen(s: &str, i: usize) -> bool {
    let b = s.as_bytes();
    b.get(i) == Some(&b'<')
        && b.get(i + 1) == Some(&b'/')
        && b.get(i + 2).is_some_and(|c| c.is_ascii_alphabetic())
}

/// locatetagend.match(rawdata, i) -> end, checking the trailing '>'.
fn whole_start_tag_end(raw: &str, i: usize) -> Option<usize> {
    let m = LOCATETAGEND.find_from_pos(raw, i).ok()??;
    if m.start() != i {
        return None;
    }
    let j = m.end();
    if raw.as_bytes().get(j.wrapping_sub(1)) != Some(&b'>') {
        return None;
    }
    Some(j)
}

/// tagfind_tolerant.match(rawdata, i): (tag, end)
fn tagfind_tolerant(raw: &str, i: usize) -> (String, usize) {
    let b = raw.as_bytes();
    let mut j = i;
    if b.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
        j += 1;
        while j < b.len() && !b" \t\n\r\x0C/>".contains(&b[j]) {
            j += 1;
        }
    }
    let tag = raw[i..j].to_string();
    // consume (?:[\t\n\r\f ]|/(?!>))*
    loop {
        if b.get(j).is_some_and(|c| b" \t\n\r\x0C".contains(c)) {
            j += 1;
            continue;
        }
        if b.get(j) == Some(&b'/') && b.get(j + 1) != Some(&b'>') {
            j += 1;
            continue;
        }
        break;
    }
    (tag, j)
}

/// entityref: '&([a-zA-Z][-.a-zA-Z0-9]*)[^a-zA-Z0-9]' -> (end, name)
fn entityref_match(s: &str, i: usize) -> Option<(usize, String)> {
    let b = s.as_bytes();
    if b.get(i) != Some(&b'&') {
        return None;
    }
    let mut j = i + 1;
    if !b.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    j += 1;
    while b.get(j).is_some_and(|c| {
        c.is_ascii_alphanumeric() || *c == b'-' || *c == b'.'
    }) {
        j += 1;
    }
    if j >= b.len() {
        return None;
    }
    // trailing [^a-zA-Z0-9] is part of the match
    let name = s[i + 1..j].to_string();
    Some((j + 1, name))
}

/// charref: '&#(?:[0-9]+|[xX][0-9a-fA-F]+)[^0-9a-fA-F]' -> (end, name)
fn charref_match(s: &str, i: usize) -> Option<(usize, String)> {
    let b = s.as_bytes();
    if b.get(i) != Some(&b'&') || b.get(i + 1) != Some(&b'#') {
        return None;
    }
    let mut j = i + 2;
    let start = j;
    let hex = b.get(j).is_some_and(|c| *c == b'x' || *c == b'X');
    if hex {
        j += 1;
    }
    let mut digits = 0usize;
    while j < b.len()
        && (if hex {
            is_hex(b[j])
        } else {
            b[j].is_ascii_digit()
        })
    {
        j += 1;
        digits += 1;
    }
    if digits == 0 || j >= b.len() || is_hex(b[j]) {
        return None;
    }
    let name = s[start..j].to_string();
    Some((j + 1, name))
}

fn incomplete_charref_match(s: &str, i: usize) -> bool {
    let b = s.as_bytes();
    if b.get(i) != Some(&b'&') || b.get(i + 1) != Some(&b'#') {
        return false;
    }
    let j = i + 2;
    if b.get(j).is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    if b.get(j).is_some_and(|c| *c == b'x' || *c == b'X') {
        return b.get(j + 1).is_some_and(|c| is_hex(*c));
    }
    false
}

/// incomplete: '&[a-zA-Z#]'
fn incomplete_match(s: &str, i: usize) -> bool {
    let b = s.as_bytes();
    b.get(i) == Some(&b'&')
        && b.get(i + 1)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'#')
}

fn is_hex(c: u8) -> bool {
    c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || (b'A'..=b'F').contains(&c)
}

// -- attribute value unescaping (attr_charref) --

/// `_unescape_attrvalue`: replaces charrefs in attribute values.
/// Port of `html.parser._replace_attr_charref` + `html.unescape` numeric part.
fn unescape_attrvalue(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'&' {
            // attr_charref: &(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*)[;=]?
            if let Some((ref_end, is_numeric, name)) = attr_charref_at(s, i) {
                if is_numeric {
                    // ref.startswith('&#'): always unescaped; a trailing '='
                    // survives (html.unescape consumes digits + optional ';'
                    // only), a trailing ';' is consumed.
                    let mut repl = unescape_numeric(name);
                    if b.get(ref_end - 1) == Some(&b'=') {
                        repl.push('=');
                    }
                    out.push_str(&repl);
                } else {
                    let trailing = b.get(ref_end - 1);
                    if trailing == Some(&b'=') {
                        // followed by an equals sign: never unescaped
                        out.push_str(&s[i..ref_end]);
                    } else if let Some(v) = crate::entities::HTML5_ENTITIES.get(name) {
                        out.push_str(v);
                    } else {
                        out.push_str(&s[i..ref_end]);
                    }
                }
                i = ref_end;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Returns (end_of_match, is_numeric, name) for a charref starting at `&`.
fn attr_charref_at(s: &str, i: usize) -> Option<(usize, bool, &str)> {
    let b = s.as_bytes();
    if b.get(i) != Some(&b'&') {
        return None;
    }
    let mut j = i + 1;
    let numeric = b.get(j) == Some(&b'#');
    if numeric {
        j += 1;
        let hex = b.get(j).is_some_and(|c| *c == b'x' || *c == b'X');
        if hex {
            j += 1;
        }
        let start = j;
        while j < b.len()
            && (if hex {
                is_hex(b[j])
            } else {
                b[j].is_ascii_digit()
            })
        {
            j += 1;
        }
        if j == start {
            return None;
        }
    } else {
        if !b.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
            return None;
        }
        j += 1;
        while b.get(j).is_some_and(|c| c.is_ascii_alphanumeric()) {
            j += 1;
        }
    }
    // optional [;=]
    if b.get(j).is_some_and(|c| *c == b';' || *c == b'=') {
        j += 1;
    }
    let name = &s[i + 1..if b.get(j - 1).is_some_and(|c| *c == b';' || *c == b'=') {
        j - 1
    } else {
        j
    }];
    Some((j, numeric, name))
}

// _invalid_charrefs (html/__init__.py): 0x00..0x9F replacements
const INVALID_CHARREFS: [(u32, char); 32] = [
    (0x00, '\u{FFFD}'),
    (0x0D, '\r'),
    (0x80, '\u{20AC}'),
    (0x81, '\u{0081}'),
    (0x82, '\u{201A}'),
    (0x83, '\u{0192}'),
    (0x84, '\u{201E}'),
    (0x85, '\u{2026}'),
    (0x86, '\u{2020}'),
    (0x87, '\u{2021}'),
    (0x88, '\u{02C6}'),
    (0x89, '\u{2030}'),
    (0x8A, '\u{0160}'),
    (0x8B, '\u{2039}'),
    (0x8C, '\u{0152}'),
    (0x8D, '\u{008D}'),
    (0x8E, '\u{017D}'),
    (0x8F, '\u{008F}'),
    (0x90, '\u{0090}'),
    (0x91, '\u{2018}'),
    (0x92, '\u{2019}'),
    (0x93, '\u{201C}'),
    (0x94, '\u{201D}'),
    (0x95, '\u{2022}'),
    (0x96, '\u{2013}'),
    (0x97, '\u{2014}'),
    (0x98, '\u{02DC}'),
    (0x99, '\u{2122}'),
    (0x9A, '\u{0161}'),
    (0x9B, '\u{203A}'),
    (0x9C, '\u{0153}'),
    (0x9D, '\u{009D}'),
];

// _invalid_codepoints (html/__init__.py): replaced with '' (sorted)
const INVALID_CODEPOINTS: [u32; 126] = [
    0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0xB, 0xE, 0xF, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
    0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x7F, 0x80, 0x81, 0x82, 0x83, 0x84,
    0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F, 0x90, 0x91, 0x92, 0x93, 0x94,
    0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D, 0x9E, 0x9F, 0xFDD0, 0xFDD1, 0xFDD2,
    0xFDD3, 0xFDD4, 0xFDD5, 0xFDD6, 0xFDD7, 0xFDD8, 0xFDD9, 0xFDDA, 0xFDDB, 0xFDDC, 0xFDDD, 0xFDDE,
    0xFDDF, 0xFDE0, 0xFDE1, 0xFDE2, 0xFDE3, 0xFDE4, 0xFDE5, 0xFDE6, 0xFDE7, 0xFDE8, 0xFDE9, 0xFDEA,
    0xFDEB, 0xFDEC, 0xFDED, 0xFDEE, 0xFDEF, 0xFFFE, 0xFFFF, 0x1FFFE, 0x1FFFF, 0x2FFFE, 0x2FFFF,
    0x3FFFE, 0x3FFFF, 0x4FFFE, 0x4FFFF, 0x5FFFE, 0x5FFFF, 0x6FFFE, 0x6FFFF, 0x7FFFE, 0x7FFFF,
    0x8FFFE, 0x8FFFF, 0x9FFFE, 0x9FFFF, 0xAFFFE, 0xAFFFF, 0xBFFFE, 0xBFFFF, 0xCFFFE, 0xCFFFF,
    0xDFFFE, 0xDFFFF, 0xEFFFE, 0xEFFFF, 0xFFFFE, 0xFFFFF, 0x10FFFE, 0x10FFFF,
];

fn unescape_numeric(name: &str) -> String {
    // name is like "#65" or "#x41" (html.unescape numeric branch, group 1
    // includes the leading '#'; trailing ';' is already stripped).
    let num = if let Some(rest) = name.strip_prefix('#') {
        if let Some(hex) = rest.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16)
        } else {
            rest.parse::<u32>()
        }
    } else {
        return "\u{FFFD}".to_string();
    };
    let Ok(num) = num else {
        return "\u{FFFD}".to_string();
    };
    if let Some((_, c)) = INVALID_CHARREFS.iter().find(|(n, _)| *n == num) {
        return c.to_string();
    }
    if (0xD800..=0xDFFF).contains(&num) || num > 0x10FFFF {
        return "\u{FFFD}".to_string();
    }
    if INVALID_CODEPOINTS.binary_search(&num).is_ok() {
        return String::new();
    }
    char::from_u32(num)
        .map(|c| c.to_string())
        .unwrap_or_else(|| "\u{FFFD}".to_string())
}
