//! The HTML2Text conversion machine: a pure-Rust port of the logic of
//! `crawl4ai/html2text/__init__.py` (class HTML2Text), minus the parts that
//! need Python (tag_callback, custom `out` sink). The PyO3 layer in lib.rs
//! owns the Tokenizer, dispatches events and flushes output.
//!
//! The Machine never calls Python; `o()` writes to `outtextlist` (internal
//! sink) or `pending_out` (external sink, flushed by the pyclass).

use crate::config::{
    BODY_WIDTH, BOLD_TEXT_STYLE_VALUES, BYPASS_TABLES, DEFAULT_IMAGE_ALT, ESCAPE_BACKSLASH,
    ESCAPE_DASH, ESCAPE_DOT, ESCAPE_PLUS, ESCAPE_SNOB, GOOGLE_LIST_INDENT, IGNORE_ANCHORS,
    IGNORE_EMPHASIS, IGNORE_IMAGES, IGNORE_MAILTO_LINKS, IGNORE_TABLES, IMAGES_AS_HTML,
    IMAGES_TO_ALT, IMAGES_WITH_SIZE, INCLUDE_SUP_SUB, INLINE_LINKS, LINKS_EACH_PARAGRAPH,
    MARK_CODE, OPEN_QUOTE, PAD_TABLES, PROTECT_LINKS, SINGLE_LINE_BREAK, SKIP_INTERNAL_LINKS,
    UNICODE_SNOB, USE_AUTOMATIC_LINKS, WRAP_LINKS, WRAP_LIST_ITEMS, WRAP_TABLES,
};
use crate::entities::{HTML5_ENTITIES, UNIFIABLE, UNIFIABLE_N};
use crate::escape::{escape_md, escape_md_section, re_space_matches, skipwrap};
use crate::style::{
    attr_get, attr_has, dumb_css_parser, element_style, google_fixed_width_font, google_has_height,
    google_list_style, google_text_emphasis, hn, list_numbering_start, prop_get,
};
use crate::tables::pad_tables_in_text;
use crate::urljoin::urljoin;
use crate::wrap;

const PLACEHOLDER: &str = "&nbsp_place_holder;";

// Python string.whitespace and string.punctuation (ASCII sets)
fn is_py_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0b}' | '\u{0c}')
}
fn is_py_punctuation(c: char) -> bool {
    "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~".contains(c)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Force {
    None,
    Truthy,
    End,
}

impl Force {
    fn truthy(self) -> bool {
        !matches!(self, Force::None)
    }
}

#[derive(Debug, Clone)]
pub struct Anchor {
    pub attrs: Vec<(String, Option<String>)>,
    pub count: usize,
    pub outcount: usize,
}

#[derive(Debug, Clone)]
pub struct List {
    pub name: String,
    pub num: usize,
}

type Attrs = Vec<(String, Option<String>)>;
type StyleProps = Vec<(String, String)>;

pub struct Machine {
    // -- config options --
    pub split_next_td: bool,
    pub td_count: usize,
    pub table_start: bool,
    pub unicode_snob: bool,
    pub escape_snob: bool,
    pub escape_backslash: bool,
    pub escape_dot: bool,
    pub escape_plus: bool,
    pub escape_dash: bool,
    pub links_each_paragraph: bool,
    pub body_width: usize,
    pub skip_internal_links: bool,
    pub inline_links: bool,
    pub protect_links: bool,
    pub google_list_indent: usize,
    pub ignore_links: bool,
    pub ignore_mailto_links: bool,
    pub ignore_images: bool,
    pub images_as_html: bool,
    pub images_to_alt: bool,
    pub images_with_size: bool,
    pub ignore_emphasis: bool,
    pub bypass_tables: bool,
    pub ignore_tables: bool,
    pub google_doc: bool,
    pub ul_item_mark: String,
    pub emphasis_mark: String,
    pub strong_mark: String,
    pub single_line_break: bool,
    pub use_automatic_links: bool,
    pub hide_strikethrough: bool,
    pub mark_code: bool,
    pub wrap_list_items: bool,
    pub wrap_links: bool,
    pub wrap_tables: bool,
    pub pad_tables: bool,
    pub default_image_alt: String,
    pub open_quote: String,
    pub close_quote: String,
    pub include_sup_sub: bool,

    // -- runtime state --
    pub outtextlist: Vec<String>,
    pub pending_out: Vec<String>,
    pub sink_internal: bool,
    pub quiet: i64,
    pub p_p: usize,
    pub outcount: usize,
    pub start: bool,
    pub space: bool,
    pub a: Vec<Anchor>,
    pub astack: Vec<Option<Attrs>>,
    pub maybe_automatic_link: Option<String>,
    pub empty_link: bool,
    pub acount: usize,
    pub list: Vec<List>,
    pub blockquote: usize,
    pub pre: bool,
    pub startpre: bool,
    pub code: bool,
    pub quote: bool,
    pub br_toggle: String,
    pub last_was_nl: bool,
    pub last_was_list: bool,
    pub style: i64,
    pub style_def: Vec<(String, StyleProps)>,
    pub tag_stack: Vec<(String, Attrs, StyleProps)>,
    pub emphasis: i64,
    pub drop_white_space: usize,
    pub inheader: bool,
    pub abbr_title: Option<String>,
    pub abbr_data: Option<String>,
    pub abbr_list: Vec<(String, String)>,
    pub baseurl: String,
    pub stressed: bool,
    pub preceding_stressed: bool,
    pub preceding_data: String,
    pub current_tag: String,
    pub inside_link: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        Machine {
            split_next_td: false,
            td_count: 0,
            table_start: false,
            unicode_snob: UNICODE_SNOB,
            escape_snob: ESCAPE_SNOB,
            escape_backslash: ESCAPE_BACKSLASH,
            escape_dot: ESCAPE_DOT,
            escape_plus: ESCAPE_PLUS,
            escape_dash: ESCAPE_DASH,
            links_each_paragraph: LINKS_EACH_PARAGRAPH,
            body_width: BODY_WIDTH,
            skip_internal_links: SKIP_INTERNAL_LINKS,
            inline_links: INLINE_LINKS,
            protect_links: PROTECT_LINKS,
            google_list_indent: GOOGLE_LIST_INDENT,
            ignore_links: IGNORE_ANCHORS,
            ignore_mailto_links: IGNORE_MAILTO_LINKS,
            ignore_images: IGNORE_IMAGES,
            images_as_html: IMAGES_AS_HTML,
            images_to_alt: IMAGES_TO_ALT,
            images_with_size: IMAGES_WITH_SIZE,
            ignore_emphasis: IGNORE_EMPHASIS,
            bypass_tables: BYPASS_TABLES,
            ignore_tables: IGNORE_TABLES,
            google_doc: false,
            ul_item_mark: "*".to_string(),
            emphasis_mark: "_".to_string(),
            strong_mark: "**".to_string(),
            single_line_break: SINGLE_LINE_BREAK,
            use_automatic_links: USE_AUTOMATIC_LINKS,
            hide_strikethrough: false,
            mark_code: MARK_CODE,
            wrap_list_items: WRAP_LIST_ITEMS,
            wrap_links: WRAP_LINKS,
            wrap_tables: WRAP_TABLES,
            pad_tables: PAD_TABLES,
            default_image_alt: DEFAULT_IMAGE_ALT.to_string(),
            open_quote: OPEN_QUOTE.to_string(),
            close_quote: crate::config::CLOSE_QUOTE.to_string(),
            include_sup_sub: INCLUDE_SUP_SUB,

            outtextlist: Vec::new(),
            pending_out: Vec::new(),
            sink_internal: true,
            quiet: 0,
            p_p: 0,
            outcount: 0,
            start: true,
            space: false,
            a: Vec::new(),
            astack: Vec::new(),
            maybe_automatic_link: None,
            empty_link: false,
            acount: 0,
            list: Vec::new(),
            blockquote: 0,
            pre: false,
            startpre: false,
            code: false,
            quote: false,
            br_toggle: String::new(),
            last_was_nl: false,
            last_was_list: false,
            style: 0,
            style_def: Vec::new(),
            tag_stack: Vec::new(),
            emphasis: 0,
            drop_white_space: 0,
            inheader: false,
            abbr_title: None,
            abbr_data: None,
            abbr_list: Vec::new(),
            baseurl: String::new(),
            stressed: false,
            preceding_stressed: false,
            preceding_data: String::new(),
            current_tag: String::new(),
            inside_link: false,
        }
    }

    pub fn set_baseurl(&mut self, baseurl: &str) {
        self.baseurl = baseurl.to_string();
    }

    /// `outtextf`: appends to outtextlist, tracks lastWasNL.
    pub fn outtextf(&mut self, s: &str) {
        self.outtextlist.push(s.to_string());
        if !s.is_empty() {
            self.last_was_nl = s.ends_with('\n');
        }
    }

    /// The sink: internal mode appends to outtextlist (and tracks lastWasNL),
    /// external mode (custom `out` callback) appends to pending_out and leaves
    /// lastWasNL untouched, exactly like Python where outtextf is never called.
    fn push_sink(&mut self, s: &str) {
        if self.sink_internal {
            self.outtextf(s);
        } else {
            self.pending_out.push(s.to_string());
        }
    }

    fn out(&mut self, data: &str, puredata: bool, force: Force) {
        if let Some(abbr) = self.abbr_data.as_mut() {
            abbr.push_str(data);
        }

        if self.quiet != 0 {
            return;
        }

        let mut data = data.to_string();
        if self.google_doc {
            // prevent white space immediately after 'begin emphasis'
            let lstripped = data.trim_start().to_string();
            if self.drop_white_space != 0 && !(self.pre || self.code) {
                data = lstripped.clone();
            }
            if !lstripped.is_empty() {
                self.drop_white_space = 0;
            }
        }

        if puredata && !self.pre {
            // Python: re.sub(r"\s+", r" ", data)
            let mut collapsed = String::with_capacity(data.len());
            let mut in_ws = false;
            for c in data.chars() {
                if c.is_whitespace() {
                    if !in_ws {
                        collapsed.push(' ');
                        in_ws = true;
                    }
                } else {
                    collapsed.push(c);
                    in_ws = false;
                }
            }
            data = collapsed;
            if data.starts_with(' ') {
                self.space = true;
                data.drain(..1);
            }
        }

        if data.is_empty() && !force.truthy() {
            return;
        }

        if self.startpre {
            if !data.starts_with('\n') && !data.starts_with("\r\n") {
                data.insert(0, '\n');
            }
            if self.mark_code {
                self.push_sink("\n[code]");
                self.p_p = 0;
            }
        }

        let mut bq = ">".repeat(self.blockquote);
        if !(force.truthy() && data.starts_with('>')) && self.blockquote != 0 {
            bq.push(' ');
        }

        if self.pre {
            if self.list.is_empty() {
                bq.push_str("    ");
            }
            bq.push_str(&"    ".repeat(self.list.len()));
            data = data.replace('\n', &format!("\n{}", bq));
        }

        if self.startpre {
            self.startpre = false;
            if !self.list.is_empty() {
                data = data.trim_start_matches('\n').to_string();
            }
        }

        if self.start {
            self.space = false;
            self.p_p = 0;
            self.start = false;
        }

        if force == Force::End {
            self.p_p = 0;
            self.push_sink("\n");
            self.space = false;
        }

        if self.p_p != 0 {
            self.push_sink(&format!(
                "{}{}",
                format!("{}\n{}", self.br_toggle, bq).repeat(self.p_p),
                ""
            ));
            self.space = false;
            self.br_toggle.clear();
        }

        if self.space {
            if !self.last_was_nl {
                self.push_sink(" ");
            }
            self.space = false;
        }

        if !self.a.is_empty()
            && ((self.p_p == 2 && self.links_each_paragraph) || force == Force::End)
        {
            if force == Force::End {
                self.push_sink("\n");
            }

            let mut newa: Vec<Anchor> = Vec::new();
            let mut emitted: Vec<(usize, String, Option<String>)> = Vec::new();
            for link in &self.a {
                if self.outcount > link.outcount {
                    let href = link
                        .attrs
                        .iter()
                        .find(|(k, _)| k == "href")
                        .map(|(_, v)| v.clone().unwrap_or_default())
                        .unwrap_or_default();
                    let title = link
                        .attrs
                        .iter()
                        .find(|(k, _)| k == "title")
                        .and_then(|(_, v)| v.clone());
                    emitted.push((link.count, href, title));
                } else {
                    newa.push(link.clone());
                }
            }
            for (count, href, title) in &emitted {
                self.push_sink(&format!("   [{}]: {}", count, urljoin(&self.baseurl, href)));
                if let Some(title) = title {
                    self.push_sink(&format!(" ({})", title));
                }
                self.push_sink("\n");
            }

            if self.a.len() != newa.len() {
                self.push_sink("\n");
            }
            self.a = newa;
        }

        if !self.abbr_list.is_empty() && force == Force::End {
            let mut lines: Vec<String> = Vec::new();
            for (abbr, definition) in &self.abbr_list {
                lines.push(format!("  *[{}]: {}\n", abbr, definition));
            }
            for line in lines {
                self.push_sink(&line);
            }
        }

        self.p_p = 0;
        self.push_sink(&data);
        self.outcount += 1;
    }

    pub fn o(&mut self, data: &str, puredata: bool, force: Force) {
        self.out(data, puredata, force);
    }

    /// `pbr`: pretty print has a line break.
    pub fn pbr(&mut self) {
        if self.p_p == 0 {
            self.p_p = 1;
        }
    }

    /// `p`: set pretty print to 1 or 2 lines.
    pub fn p(&mut self) {
        self.p_p = if self.single_line_break { 1 } else { 2 };
    }

    /// `soft_br`: soft breaks.
    pub fn soft_br(&mut self) {
        self.pbr();
        self.br_toggle = "  ".to_string();
    }

    pub fn charref(&self, name: &str) -> String {
        let c: u32 = if name.starts_with(['x', 'X']) {
            u32::from_str_radix(&name[1..], 16).unwrap_or(0)
        } else {
            name.parse::<u32>().unwrap_or(0)
        };

        if !self.unicode_snob {
            if let Some((_, v)) = UNIFIABLE_N.iter().find(|(n, _)| *n == c) {
                return v.to_string();
            }
        }
        // Python: chr(c) raises ValueError for c > 0x10FFFF -> ""
        if c > 0x10FFFF {
            return String::new();
        }
        // Python allows lone surrogates via chr(); Rust strings cannot hold
        // them, so they are replaced (documented deviation).
        match char::from_u32(c) {
            Some(ch) => ch.to_string(),
            None => "\u{FFFD}".to_string(),
        }
    }

    pub fn entityref(&self, c: &str) -> String {
        // config.UNIFIABLE["nbsp"] is set to the placeholder at __init__;
        // always emitted, replaced in finish_text
        if c == "nbsp" {
            return PLACEHOLDER.to_string();
        }
        if !self.unicode_snob {
            if let Some((_, v)) = UNIFIABLE.iter().find(|(k, _)| *k == c) {
                return v.to_string();
            }
        }
        let key = format!("{};", c);
        match HTML5_ENTITIES.get(&key) {
            Some(ch) => ch.to_string(),
            None => format!("&{};", c),
        }
    }

    pub fn google_nest_count(&self, style: &[(String, String)]) -> usize {
        if let Some(margin) = prop_get(style, "margin-left") {
            if margin.len() >= 2 {
                // int(style["margin-left"][:-2]) // google_list_indent
                if let Ok(n) = margin[..margin.len() - 2].parse::<usize>() {
                    return n / self.google_list_indent;
                }
            }
        }
        0
    }

    /// `previousIndex`: index of a matching link in self.a.
    pub fn previous_index(&self, attrs: &[(String, Option<String>)]) -> Option<usize> {
        if !attr_has(attrs, "href") {
            return None;
        }
        let href = attr_get(attrs, "href");
        for (i, a) in self.a.iter().enumerate() {
            let mut found = false;
            if attr_get(&a.attrs, "href") == href {
                if attr_has(&a.attrs, "title") || attr_has(attrs, "title") {
                    if attr_has(&a.attrs, "title")
                        && attr_has(attrs, "title")
                        && attr_get(&a.attrs, "title") == attr_get(attrs, "title")
                    {
                        found = true;
                    }
                } else {
                    found = true;
                }
            }
            if found {
                return Some(i);
            }
        }
        None
    }

    pub fn handle_emphasis(
        &mut self,
        start: bool,
        tag_style: &[(String, String)],
        parent_style: &[(String, String)],
    ) {
        let tag_emphasis = google_text_emphasis(tag_style);
        let parent_emphasis = google_text_emphasis(parent_style);

        let strikethrough =
            tag_emphasis.iter().any(|e| e == "line-through") && self.hide_strikethrough;

        let mut bold = false;
        for marker in BOLD_TEXT_STYLE_VALUES {
            if tag_emphasis.iter().any(|e| e == marker)
                && !parent_emphasis.iter().any(|e| e == marker)
            {
                bold = true;
                break;
            }
        }

        let italic = tag_emphasis.iter().any(|e| e == "italic")
            && !parent_emphasis.iter().any(|e| e == "italic");
        let fixed = google_fixed_width_font(tag_style)
            && !google_fixed_width_font(parent_style)
            && !self.pre;

        if start {
            if bold || italic || fixed {
                self.emphasis += 1;
            }
            if strikethrough {
                self.quiet += 1;
            }
            if italic {
                let mark = self.emphasis_mark.clone();
                self.o(&mark, false, Force::None);
                self.drop_white_space += 1;
            }
            if bold {
                let mark = self.strong_mark.clone();
                self.o(&mark, false, Force::None);
                self.drop_white_space += 1;
            }
            if fixed {
                self.o("`", false, Force::None);
                self.drop_white_space += 1;
                self.code = true;
            }
        } else {
            if bold || italic || fixed {
                self.emphasis -= 1;
                self.space = false;
            }
            if fixed {
                if self.drop_white_space != 0 {
                    self.drop_white_space -= 1;
                } else {
                    self.o("`", false, Force::None);
                }
                self.code = false;
            }
            if bold {
                if self.drop_white_space != 0 {
                    self.drop_white_space -= 1;
                } else {
                    let mark = self.strong_mark.clone();
                    self.o(&mark, false, Force::None);
                }
            }
            if italic {
                if self.drop_white_space != 0 {
                    self.drop_white_space -= 1;
                } else {
                    let mark = self.emphasis_mark.clone();
                    self.o(&mark, false, Force::None);
                }
            }
            if (bold || italic) && self.emphasis == 0 {
                self.o(" ", false, Force::None);
            }
            if strikethrough {
                self.quiet -= 1;
            }
        }
    }

    pub fn handle_tag(&mut self, tag: &str, mut attrs: Vec<(String, Option<String>)>, start: bool) {
        self.current_tag = tag.to_string();

        // <base> updates the base URL for relative links
        if tag == "base" && start {
            if let Some(href) = attr_get(&attrs, "href") {
                self.baseurl = href.to_string();
            }
        }

        // first thing inside the anchor tag is another tag that produces output
        if start
            && self.maybe_automatic_link.is_some()
            && !["p", "div", "style", "dl", "dt"].contains(&tag)
            && (tag != "img" || self.ignore_images)
        {
            self.o("[", false, Force::None);
            self.maybe_automatic_link = None;
            self.empty_link = false;
        }

        let mut tag_style: Vec<(String, String)> = Vec::new();
        let mut parent_style: Vec<(String, String)> = Vec::new();
        if self.google_doc {
            if start {
                if let Some(top) = self.tag_stack.last() {
                    parent_style = top.2.clone();
                }
                tag_style = element_style(&attrs, &self.style_def, &parent_style);
                self.tag_stack
                    .push((tag.to_string(), attrs.clone(), tag_style.clone()));
            } else {
                if let Some((_, a, ts)) = self.tag_stack.pop() {
                    attrs = a;
                    tag_style = ts;
                }
                if let Some(top) = self.tag_stack.last() {
                    parent_style = top.2.clone();
                }
            }
        }

        if hn(tag) != 0 {
            // check if hn is inside of an 'a' tag (incorrect but found in the wild)
            if !self.astack.is_empty() {
                if start {
                    self.inheader = true;
                    // are inside link name, so only add '#' if it can appear before '['
                    if let Some(last) = self.outtextlist.last() {
                        if last == "[" {
                            self.outtextlist.pop();
                            self.space = false;
                            self.o(&format!("{} ", "#".repeat(hn(tag))), false, Force::None);
                            self.o("[", false, Force::None);
                        }
                    }
                } else {
                    self.p_p = 0; // don't break up link name
                    self.inheader = false;
                    return; // prevent redundant emphasis marks on headers
                }
            } else {
                self.p();
                if start {
                    self.inheader = true;
                    self.o(&format!("{} ", "#".repeat(hn(tag))), false, Force::None);
                } else {
                    self.inheader = false;
                    return; // prevent redundant emphasis marks on headers
                }
            }
        }

        if tag == "p" || tag == "div" {
            if self.google_doc {
                if start && google_has_height(&tag_style) {
                    self.p();
                } else {
                    self.soft_br();
                }
            } else if !self.astack.is_empty() || self.split_next_td {
                // pass
            } else {
                self.p();
            }
        }

        if tag == "br" && start {
            if self.blockquote > 0 {
                self.o("  \n> ", false, Force::None);
            } else {
                self.o("  \n", false, Force::None);
            }
        }

        if tag == "hr" && start {
            self.p();
            self.o("* * *", false, Force::None);
            self.p();
        }

        if tag == "head" || tag == "style" || tag == "script" {
            if start {
                self.quiet += 1;
            } else {
                self.quiet -= 1;
            }
        }

        if tag == "style" {
            if start {
                self.style += 1;
            } else {
                self.style -= 1;
            }
        }

        if tag == "body" {
            self.quiet = 0; // sites like 9rules.com never close <head>
        }

        if tag == "blockquote" {
            if start {
                self.p();
                self.o("> ", false, Force::Truthy);
                self.start = true;
                self.blockquote += 1;
            } else {
                self.blockquote -= 1;
                self.p();
            }
        }

        if (tag == "em" || tag == "i" || tag == "u") && !self.ignore_emphasis {
            // Separate with a space if we immediately follow an alphanumeric
            // character, since otherwise Markdown won't render the emphasis.
            let emphasis = if start
                && !self.preceding_data.is_empty()
                && !is_py_whitespace(self.preceding_data.chars().last().unwrap())
                && !is_py_punctuation(self.preceding_data.chars().last().unwrap())
            {
                self.preceding_data.push(' ');
                format!(" {}", self.emphasis_mark)
            } else {
                self.emphasis_mark.clone()
            };
            self.o(&emphasis, false, Force::None);
            if start {
                self.stressed = true;
            }
        }

        if (tag == "strong" || tag == "b") && !self.ignore_emphasis {
            let strong = if start
                && !self.preceding_data.is_empty()
                && !self.strong_mark.is_empty()
                && self.preceding_data.chars().last().unwrap()
                    == self.strong_mark.chars().next().unwrap()
            {
                self.preceding_data.push(' ');
                format!(" {}", self.strong_mark)
            } else {
                self.strong_mark.clone()
            };
            self.o(&strong, false, Force::None);
            if start {
                self.stressed = true;
            }
        }

        if tag == "del" || tag == "strike" || tag == "s" {
            let strike = if start
                && !self.preceding_data.is_empty()
                && self.preceding_data.ends_with('~')
            {
                self.preceding_data.push(' ');
                " ~~".to_string()
            } else {
                "~~".to_string()
            };
            self.o(&strike, false, Force::None);
            if start {
                self.stressed = true;
            }
        }

        if self.google_doc && !self.inheader {
            // handle some font attributes, but leave headers clean
            self.handle_emphasis(start, &tag_style, &parent_style);
        }

        if (tag == "kbd" || tag == "code" || tag == "tt") && !self.pre {
            self.o("`", false, Force::None);
            self.code = !self.code;
        }

        if tag == "abbr" {
            if start {
                self.abbr_title = None;
                self.abbr_data = Some(String::new());
                if let Some(title) = attr_get(&attrs, "title") {
                    self.abbr_title = Some(title.to_string());
                }
            } else {
                if let Some(title) = self.abbr_title.clone() {
                    let data = self.abbr_data.clone().unwrap_or_default();
                    upsert_abbr(&mut self.abbr_list, data, title);
                    self.abbr_title = None;
                }
                self.abbr_data = None;
            }
        }

        if tag == "q" {
            if !self.quote {
                let q = self.open_quote.clone();
                self.o(&q, false, Force::None);
            } else {
                let q = self.close_quote.clone();
                self.o(&q, false, Force::None);
            }
            self.quote = !self.quote;
        }

        if tag == "a" && !self.ignore_links {
            if start {
                self.inside_link = true;
                let href = attr_get(&attrs, "href");
                if let Some(href) = href {
                    if (self.skip_internal_links && href.starts_with('#'))
                        || (self.ignore_mailto_links && href.starts_with("mailto:"))
                    {
                        self.astack.push(None);
                    } else {
                        self.maybe_automatic_link = Some(href.to_string());
                        self.empty_link = true;
                        if self.protect_links {
                            let protected = format!("<{}>", href);
                            if let Some(slot) = attrs.iter_mut().find(|(k, _)| k == "href") {
                                slot.1 = Some(protected);
                            }
                        }
                        self.astack.push(Some(attrs.clone()));
                    }
                } else {
                    self.astack.push(None);
                }
            } else {
                self.inside_link = false;
                if let Some(popped) = self.astack.pop() {
                    if self.maybe_automatic_link.is_some() && !self.empty_link {
                        self.maybe_automatic_link = None;
                    } else if let Some(a) = popped {
                        if self.empty_link {
                            self.o("[", false, Force::None);
                            self.empty_link = false;
                            self.maybe_automatic_link = None;
                        }
                        if self.inline_links {
                            self.p_p = 0;
                            let title = attr_get(&a, "title").unwrap_or("").to_string();
                            let title = escape_md(&title);
                            self.link_url(attr_get(&a, "href").unwrap_or_default(), &title);
                        } else {
                            let i = self.previous_index(&a);
                            let a_props;
                            if let Some(idx) = i {
                                a_props = self.a[idx].clone();
                            } else {
                                self.acount += 1;
                                a_props = Anchor {
                                    attrs: a.clone(),
                                    count: self.acount,
                                    outcount: self.outcount,
                                };
                                self.a.push(a_props.clone());
                            }
                            self.o(&format!("][{}]", a_props.count), false, Force::None);
                        }
                    }
                }
            }
        }

        if tag == "img" && start && !self.ignore_images {
            if let Some(src) = attr_get(&attrs, "src") {
                let src = src.to_string();
                if !self.images_to_alt {
                    // Python mutates the shared dict: attrs["href"] = attrs["src"].
                    // The a-start pushed a clone into astack, so mirror it there.
                    let upsert_href = |v: &mut Vec<(String, Option<String>)>| {
                        if let Some(slot) = v.iter_mut().find(|(k, _)| k == "href") {
                            slot.1 = Some(src.clone());
                        } else {
                            v.push(("href".to_string(), Some(src.clone())));
                        }
                    };
                    upsert_href(&mut attrs);
                    if let Some(Some(top)) = self.astack.last_mut() {
                        upsert_href(top);
                    }
                }
                let alt = attr_get(&attrs, "alt")
                    .map(str::to_string)
                    .unwrap_or_else(|| self.default_image_alt.clone());

                if self.images_as_html
                    || (self.images_with_size
                        && (attr_has(&attrs, "width") || attr_has(&attrs, "height")))
                {
                    self.o(&format!("<img src='{}' ", src), false, Force::None);
                    if let Some(w) = attr_get(&attrs, "width") {
                        self.o(&format!("width='{}' ", w), false, Force::None);
                    }
                    if let Some(h) = attr_get(&attrs, "height") {
                        self.o(&format!("height='{}' ", h), false, Force::None);
                    }
                    if !alt.is_empty() {
                        self.o(&format!("alt='{}' ", alt), false, Force::None);
                    }
                    self.o("/>", false, Force::None);
                    return;
                }

                if let Some(href) = self.maybe_automatic_link.clone() {
                    if self.images_to_alt && escape_md(&alt) == href && is_absolute_url(&href) {
                        self.o(&format!("<{}>", escape_md(&alt)), false, Force::None);
                        self.empty_link = false;
                        return;
                    } else {
                        self.o("[", false, Force::None);
                        self.maybe_automatic_link = None;
                        self.empty_link = false;
                    }
                }

                if self.images_to_alt {
                    self.o(&escape_md(&alt), false, Force::None);
                } else {
                    self.o(&format!("![{}]", escape_md(&alt)), false, Force::None);
                    if self.inline_links {
                        let href = attr_get(&attrs, "href").unwrap_or("");
                        self.o(
                            &format!("({})", escape_md(&urljoin(&self.baseurl, href))),
                            false,
                            Force::None,
                        );
                    } else {
                        let i = self.previous_index(&attrs);
                        let a_props;
                        if let Some(idx) = i {
                            a_props = self.a[idx].clone();
                        } else {
                            self.acount += 1;
                            a_props = Anchor {
                                attrs: attrs.clone(),
                                count: self.acount,
                                outcount: self.outcount,
                            };
                            self.a.push(a_props.clone());
                        }
                        self.o(&format!("[{}]", a_props.count), false, Force::None);
                    }
                }
            }
        }

        if tag == "dl" && start {
            self.p();
            self.p_p = 0;
        } else if tag == "dt" && start {
            if self.p_p == 0 {
                self.o("\n\n", false, Force::None);
            }
            self.p_p = 0;
        } else if tag == "dt" && !start {
            self.o("\n", false, Force::None);
        } else if tag == "dd" && start {
            self.o("    ", false, Force::None);
        } else if tag == "dd" && !start {
            self.p_p = 0;
        }

        if tag == "ol" || tag == "ul" {
            // Google Docs create sub lists as top level lists
            if self.list.is_empty() && !self.last_was_list {
                self.p();
            }
            if start {
                let list_style = if self.google_doc {
                    google_list_style(&tag_style).to_string()
                } else {
                    tag.to_string()
                };
                let numbering_start = list_numbering_start(&attrs);
                self.list.push(List {
                    name: list_style,
                    num: numbering_start,
                });
            } else {
                if !self.list.is_empty() {
                    self.list.pop();
                    if !self.google_doc && self.list.is_empty() {
                        self.o("\n", false, Force::None);
                    }
                }
            }
            self.last_was_list = true;
        } else {
            self.last_was_list = false;
        }

        if tag == "li" {
            self.pbr();
            if start {
                let li = if let Some(top) = self.list.last() {
                    top.clone()
                } else {
                    List {
                        name: "ul".to_string(),
                        num: 0,
                    }
                };
                if self.google_doc {
                    self.o(
                        &"  ".repeat(self.google_nest_count(&tag_style)),
                        false,
                        Force::None,
                    );
                } else {
                    // Indent two spaces per list, except use three spaces for
                    // an unordered list inside an ordered list.
                    let mut indents: Vec<String> = Vec::new();
                    let mut parent_list: Option<String> = None;
                    for l in &self.list {
                        indents.push(if parent_list.as_deref() == Some("ol") && l.name == "ul" {
                            "   ".to_string()
                        } else {
                            "  ".to_string()
                        });
                        parent_list = Some(l.name.clone());
                    }
                    for ind in &indents {
                        self.o(ind, false, Force::None);
                    }
                }

                if li.name == "ul" {
                    self.o(&format!("{} ", self.ul_item_mark), false, Force::None);
                } else if li.name == "ol" {
                    let mut num = li.num;
                    num += 1;
                    if let Some(top) = self.list.last_mut() {
                        top.num = num;
                    }
                    self.o(&format!("{}. ", num), false, Force::None);
                }
                self.start = true;
            }
        }

        if tag == "caption" && !start {
            // Ensure caption text ends on its own line before table rows
            self.soft_br();
        }

        if tag == "table" || tag == "tr" || tag == "td" || tag == "th" {
            if self.ignore_tables {
                if tag == "tr" && !start {
                    self.soft_br();
                }
            } else if self.bypass_tables {
                if start {
                    self.soft_br();
                    let mut attr_str = String::new();
                    for (k, v) in &attrs {
                        match v {
                            Some(v) => attr_str.push_str(&format!(" {}=\"{}\"", k, v)),
                            None => attr_str.push_str(&format!(" {}", k)),
                        }
                    }
                    if tag == "td" || tag == "th" {
                        self.o(&format!("<{}{}>\n\n", tag, attr_str), false, Force::None);
                    } else {
                        self.o(&format!("<{}{}>", tag, attr_str), false, Force::None);
                    }
                } else {
                    if tag == "td" || tag == "th" {
                        self.o(&format!("\n</{}>", tag), false, Force::None);
                    } else {
                        self.o(&format!("</{}>", tag), false, Force::None);
                    }
                }
            } else {
                if tag == "table" {
                    if start {
                        self.table_start = true;
                        if self.pad_tables {
                            self.o(
                                &format!("<{}>", crate::config::TABLE_MARKER_FOR_PAD),
                                false,
                                Force::None,
                            );
                            self.o("  \n", false, Force::None);
                        } else {
                            // Ensure table starts on its own line (GFM requirement)
                            self.soft_br();
                        }
                    } else {
                        if self.pad_tables {
                            // add break in case the table is empty or its 1 row table
                            self.soft_br();
                            self.o(
                                &format!("</{}>", crate::config::TABLE_MARKER_FOR_PAD),
                                false,
                                Force::None,
                            );
                            self.o("  \n", false, Force::None);
                        }
                    }
                }
                if (tag == "td" || tag == "th") && start {
                    if self.pad_tables {
                        if self.split_next_td {
                            self.o("| ", false, Force::None);
                        }
                    } else {
                        if self.split_next_td {
                            self.o(" | ", false, Force::None);
                        } else {
                            self.o("| ", false, Force::None);
                        }
                    }
                    self.split_next_td = true;
                }
                if tag == "tr" && start {
                    self.td_count = 0;
                }
                if tag == "tr" && !start {
                    if !self.pad_tables {
                        // Add trailing pipe for GFM compliance
                        self.o(" |", false, Force::None);
                    }
                    self.split_next_td = false;
                    self.soft_br();
                }
                if tag == "tr" && !start && self.table_start {
                    if self.pad_tables {
                        // pad_tables: plain separator (post-processor reformats)
                        self.o(
                            &vec!["---"; self.td_count].join("|").to_string(),
                            false,
                            Force::None,
                        );
                    } else {
                        // GFM: separator with leading/trailing pipes
                        self.o(
                            &format!("| {} |", vec!["---"; self.td_count].join(" | ")),
                            false,
                            Force::None,
                        );
                    }
                    self.soft_br();
                    self.table_start = false;
                }
                if (tag == "td" || tag == "th") && start {
                    self.td_count += 1;
                }
            }
        }

        if tag == "pre" {
            if start {
                self.startpre = true;
                self.pre = true;
            } else {
                self.pre = false;
                if self.mark_code {
                    self.push_sink("\n[/code]");
                }
            }
            self.p();
        }

        if (tag == "sup" || tag == "sub") && self.include_sup_sub {
            if start {
                self.o(&format!("<{}>", tag), false, Force::None);
            } else {
                self.o(&format!("</{}>", tag), false, Force::None);
            }
        }
    }

    fn link_url(&mut self, link: &str, title: &str) {
        let url = urljoin(&self.baseurl, link);
        let title_part = if title.trim().is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", title)
        };
        self.o(
            &format!("]({}{})", escape_md(&url), title_part),
            false,
            Force::None,
        );
    }

    pub fn handle_data(&mut self, data: &str, entity_char: bool) {
        if data.is_empty() {
            // Data may be empty for some HTML entities. For example,
            // LEFT-TO-RIGHT MARK.
            return;
        }

        let mut data = data.to_string();
        if self.stressed {
            data = data.trim().to_string();
            self.stressed = false;
            self.preceding_stressed = true;
        } else if self.preceding_stressed {
            let first = data.chars().next().unwrap();
            // re.match(r"[^][(){}\s.!?]", data[0])
            let is_skip = "] [ ( ) { } \\s . ! ?".contains(first) || first.is_whitespace();
            if !is_skip
                && hn(&self.current_tag) == 0
                && !["a", "code", "pre"].contains(&self.current_tag.as_str())
            {
                // should match a letter or common punctuation
                data.insert(0, ' ');
            }
            self.preceding_stressed = false;
        }

        if self.style > 0 {
            let parsed = dumb_css_parser(&data);
            for (sel, props) in parsed {
                if let Some(slot) = self.style_def.iter_mut().find(|(s, _)| *s == sel) {
                    slot.1 = props;
                } else {
                    self.style_def.push((sel, props));
                }
            }
        }

        if let Some(href) = self.maybe_automatic_link.clone() {
            if href == data && is_absolute_url(&href) && self.use_automatic_links {
                self.o(&format!("<{}>", data), false, Force::None);
                self.empty_link = false;
                return;
            } else {
                self.o("[", false, Force::None);
                self.maybe_automatic_link = None;
                self.empty_link = false;
            }
        }

        if !self.code && !self.pre && !entity_char {
            // escape_backslash defaults to True in utils.escape_md_section
            data = escape_md_section(
                &data,
                true,
                self.escape_snob,
                self.escape_dot,
                self.escape_plus,
                self.escape_dash,
            );
        }
        self.preceding_data = data.clone();
        self.o(&data, true, Force::None);
    }

    /// join outtextlist, replace the nbsp placeholder, clear the list.
    pub fn finish_text(&mut self) -> String {
        let outtext: String = self.outtextlist.concat();
        let nbsp = if self.unicode_snob { "\u{a0}" } else { " " };
        let outtext = outtext.replace(PLACEHOLDER, nbsp);
        self.outtextlist.clear();
        outtext
    }

    /// Port of `optwrap` (textwrap-based paragraph wrapping).
    pub fn optwrap(&mut self, text: &str) -> String {
        if self.body_width == 0 {
            return text.to_string();
        }

        let mut result = String::new();
        let mut newlines = 0usize;
        // To avoid the non-wrap behaviour for entire paras
        // because of the presence of a link in it
        if !self.wrap_links {
            self.inline_links = false;
        }
        for para in text.split('\n') {
            if !para.is_empty() {
                if !skipwrap(
                    para,
                    self.wrap_links,
                    self.wrap_list_items,
                    self.wrap_tables,
                ) {
                    let indent: &str = if para.starts_with(&format!("  {}", self.ul_item_mark)) {
                        // list item continuation: add a double indent to the new lines
                        "    "
                    } else if para.starts_with("> ") {
                        // blockquote continuation: add the greater than symbol
                        "> "
                    } else {
                        ""
                    };
                    let wrapped = wrap::wrap(para, self.body_width, indent);
                    result.push_str(&wrapped.join("\n"));
                    if para.ends_with("  ") {
                        result.push_str("  \n");
                        newlines = 1;
                    } else if !indent.is_empty() {
                        result.push('\n');
                        newlines = 1;
                    } else {
                        result.push_str("\n\n");
                        newlines = 2;
                    }
                } else {
                    if !re_space_matches(para) {
                        result.push_str(para);
                        result.push('\n');
                        newlines = 1;
                    }
                }
            } else {
                if newlines < 2 {
                    result.push('\n');
                    newlines += 1;
                }
            }
        }
        result
    }

    /// pad_tables_in_text wrapper (post-processor).
    pub fn pad_tables(&self, text: &str) -> String {
        pad_tables_in_text(text, 1)
    }
}

fn upsert_abbr(list: &mut Vec<(String, String)>, data: String, title: String) {
    if let Some(slot) = list.iter_mut().find(|(k, _)| *k == data) {
        slot.1 = title;
    } else {
        list.push((data, title));
    }
}

/// absolute_url_matcher: ^[a-zA-Z+]+://
pub fn is_absolute_url(s: &str) -> bool {
    let mut i = 0;
    for c in s.chars() {
        if c.is_ascii_alphabetic() || c == '+' {
            i += 1;
        } else {
            break;
        }
    }
    i > 0 && s[i..].starts_with("://")
}
