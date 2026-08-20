//! CSS helpers mirroring crawl4ai/html2text/utils.py (style-related parts)

/// Map of css selector -> properties, from a <style> block.
pub fn dumb_css_parser(data: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut d = String::with_capacity(data.len() + 1);
    d.push_str(data);
    d.push(';');

    // remove @import sentences
    let mut import_index = d.find("@import");
    while let Some(idx) = import_index {
        let end = d[idx..].find(';').map(|p| idx + p + 1);
        match end {
            Some(e) => {
                d.drain(idx..e);
            }
            None => break,
        }
        import_index = d.find("@import");
    }

    let mut elements: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut bad = false;
    for part in d.split('}') {
        if !part.contains('{') {
            continue;
        }
        let pieces: Vec<&str> = part.split('{').collect();
        if pieces.len() != 2 {
            // a selector containing multiple '{' makes the Python dict
            // comprehension raise ValueError -> the WHOLE stylesheet is dropped
            bad = true;
            break;
        }
        let selector = pieces[0].trim().to_string();
        let props = dumb_property_dict(pieces[1]);
        elements.push((selector, props));
    }
    if bad {
        return Vec::new();
    }
    // dict semantics: a later duplicate selector overrides the earlier one
    let mut deduped: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for (sel, props) in elements {
        if let Some(slot) = deduped.iter_mut().find(|(s, _)| *s == sel) {
            slot.1 = props;
        } else {
            deduped.push((sel, props));
        }
    }
    deduped
}

/// :returns: A hash of css attributes
pub fn dumb_property_dict(style: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for z in style.split(';') {
        if let Some(colon) = z.find(':') {
            let x = z[..colon].trim().to_lowercase();
            let y = z[colon + 1..].trim().to_lowercase();
            out.push((x, y));
        }
    }
    out
}

pub fn prop_get<'a>(props: &'a [(String, String)], key: &str) -> Option<&'a str> {
    props
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

pub fn style_has(props: &[(String, String)], key: &str) -> bool {
    props.iter().any(|(k, _)| k == key)
}

/// Compute the "final" style of an element: parent + class styles + inline style.
pub fn element_style(
    attrs: &[(String, Option<String>)],
    style_def: &[(String, Vec<(String, String)>)],
    parent_style: &[(String, String)],
) -> Vec<(String, String)> {
    let mut style: Vec<(String, String)> = parent_style.to_vec();

    if let Some(class) = attr_get(attrs, "class") {
        for css_class in class.split_whitespace() {
            let sel = format!(".{}", css_class);
            if let Some((_, css_style)) = style_def.iter().find(|(s, _)| *s == sel) {
                for (k, v) in css_style {
                    upsert(&mut style, k, v);
                }
            }
        }
    }
    if let Some(inline) = attr_get(attrs, "style") {
        let immediate = dumb_property_dict(inline);
        for (k, v) in immediate {
            upsert(&mut style, &k, &v);
        }
    }
    style
}

fn upsert(style: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(slot) = style.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value.to_string();
    } else {
        style.push((key.to_string(), value.to_string()));
    }
}

pub fn attr_get<'a>(attrs: &'a [(String, Option<String>)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_deref())
}

/// Key presence, regardless of value (Python `key in attrs`).
pub fn attr_has(attrs: &[(String, Option<String>)], key: &str) -> bool {
    attrs.iter().any(|(k, _)| k == key)
}

/// Finds out whether this is an ordered or unordered list.
pub fn google_list_style(style: &[(String, String)]) -> &'static str {
    if let Some(ls) = prop_get(style, "list-style-type") {
        if ["disc", "circle", "square", "none"].contains(&ls) {
            return "ul";
        }
    }
    "ol"
}

/// Check if the style has the 'height' attribute explicitly defined.
pub fn google_has_height(style: &[(String, String)]) -> bool {
    style_has(style, "height")
}

/// A list of all emphasis modifiers of the element.
pub fn google_text_emphasis(style: &[(String, String)]) -> Vec<String> {
    let mut emphasis = Vec::new();
    if let Some(v) = prop_get(style, "text-decoration") {
        emphasis.push(v.to_string());
    }
    if let Some(v) = prop_get(style, "font-style") {
        emphasis.push(v.to_string());
    }
    if let Some(v) = prop_get(style, "font-weight") {
        emphasis.push(v.to_string());
    }
    emphasis
}

/// Check if the css of the current element defines a fixed width font.
pub fn google_fixed_width_font(style: &[(String, String)]) -> bool {
    let font_family = prop_get(style, "font-family").unwrap_or("");
    font_family == "courier new" || font_family == "consolas"
}

/// Extract numbering from list element attributes.
pub fn list_numbering_start(attrs: &[(String, Option<String>)]) -> usize {
    if let Some(start) = attr_get(attrs, "start") {
        if let Ok(n) = start.parse::<usize>() {
            return n.saturating_sub(1);
        }
    }
    0
}

/// Heading level (1-9) if `tag` is hN.
pub fn hn(tag: &str) -> usize {
    if tag.starts_with('h') && tag.len() == 2 {
        let n = tag.as_bytes()[1];
        if n > b'0' && n <= b'9' {
            return (n - b'0') as usize;
        }
    }
    0
}
