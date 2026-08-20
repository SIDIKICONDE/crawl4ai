//! Port of CPython `urllib/parse.py` urljoin (as used by html2text's link
//! handling). Only the pieces reachable from urljoin are ported:
//! `_urlsplit`, `_urlunsplit`, `uses_relative`, `uses_netloc`.

// schemes in urllib.parse.uses_relative
const USES_RELATIVE: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms",
    "prospero", "rtsp", "rtsps", "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];

// schemes in urllib.parse.uses_netloc
const USES_NETLOC: &[&str] = &[
    "",
    "ftp",
    "http",
    "gopher",
    "nntp",
    "telnet",
    "imap",
    "wais",
    "file",
    "mms",
    "https",
    "shttp",
    "snews",
    "prospero",
    "rtsp",
    "rtsps",
    "rtspu",
    "rsync",
    "svn",
    "svn+ssh",
    "sftp",
    "nfs",
    "git",
    "git+ssh",
    "ws",
    "wss",
    "itms-services",
];

const SCHEME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-.";

// _WHATWG_C0_CONTROL_OR_SPACE (used for lstrip)
const C0_CONTROL_OR_SPACE: &[char] = &[
    '\u{00}', '\u{01}', '\u{02}', '\u{03}', '\u{04}', '\u{05}', '\u{06}', '\u{07}', '\u{08}', '\t',
    '\n', '\u{0b}', '\u{0c}', '\r', '\u{0e}', '\u{0f}', '\u{10}', '\u{11}', '\u{12}', '\u{13}',
    '\u{14}', '\u{15}', '\u{16}', '\u{17}', '\u{18}', '\u{19}', '\u{1a}', '\u{1b}', '\u{1c}',
    '\u{1d}', '\u{1e}', '\u{1f}', ' ',
];

// _UNSAFE_URL_BYTES_TO_REMOVE
const UNSAFE_BYTES: &[char] = &['\t', '\n', '\r', '\u{00}'];

/// Removes \t \n \r \0 from a string (bytes-wise in CPython).
fn remove_unsafe(s: &str) -> String {
    if !s.contains(['\t', '\n', '\r', '\u{0}']) {
        return s.to_string();
    }
    s.chars().filter(|c| !UNSAFE_BYTES.contains(c)).collect()
}

struct Split {
    scheme: Option<String>,
    netloc: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
    /// the processed input url (lstrip + unsafe-byte removal), as returned
    /// by `_coerce_result(url)` in the early-return paths of urljoin
    processed: String,
}

/// `_urlsplit(url, None, allow_fragments)` for str input.
fn urlsplit(url: &str, allow_fragments: bool) -> Split {
    let mut url: String = url
        .trim_start_matches(|c| C0_CONTROL_OR_SPACE.contains(&c))
        .to_string();
    url = remove_unsafe(&url);

    let mut scheme: Option<String> = None;
    let mut netloc: Option<String> = None;
    let mut query: Option<String> = None;
    let mut fragment: Option<String> = None;
    let processed = url.clone();

    if let Some(i) = url.find(':') {
        if i > 0 && url.as_bytes()[0].is_ascii_alphabetic() {
            let head = &url[..i];
            if head.chars().all(|c| SCHEME_CHARS.contains(c)) {
                scheme = Some(head.to_ascii_lowercase());
                url = url[i + 1..].to_string();
            }
        }
    }

    if url.starts_with("//") {
        // _splitnetloc(url, 2)
        let mut delim = url.len();
        for c in ['/', '?', '#'] {
            if let Some(p) = url[2..].find(c) {
                delim = delim.min(2 + p);
            }
        }
        let (nl, rest) = url.split_at(delim);
        netloc = Some(nl[2..].to_string());
        url = rest.to_string();
        // (CPython validates IPv6 brackets here and may raise; html2text
        //  never feeds such URLs, so this is skipped.)
    }

    if allow_fragments {
        if let Some(p) = url.find('#') {
            fragment = Some(url[p + 1..].to_string());
            url.truncate(p);
        }
    }
    if let Some(p) = url.find('?') {
        query = Some(url[p + 1..].to_string());
        url.truncate(p);
    }

    Split {
        scheme,
        netloc,
        path: url,
        query,
        fragment,
        processed,
    }
}

/// `_urlunsplit(scheme, netloc, url, query, fragment)`.
fn urlunsplit(
    scheme: Option<&str>,
    netloc: Option<&str>,
    url: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut url = url.to_string();
    match netloc {
        Some(nl) => {
            if !url.is_empty() && !url.starts_with('/') {
                url = format!("/{}", url);
            }
            url = format!("//{}{}", nl, url);
        }
        None => {
            if url.starts_with("//") {
                url = format!("//{}", url);
            }
        }
    }
    if let Some(s) = scheme {
        if !s.is_empty() {
            url = format!("{}:{}", s, url);
        }
    }
    if let Some(q) = query {
        url = format!("{}?{}", url, q);
    }
    if let Some(f) = fragment {
        url = format!("{}#{}", url, f);
    }
    url
}

/// Port of `urllib.parse.urljoin(base, url)`.
pub fn urljoin(base: &str, url: &str) -> String {
    if base.is_empty() {
        return url.to_string();
    }
    if url.is_empty() {
        return base.to_string();
    }

    let b = urlsplit(base, true);
    let mut u = urlsplit(url, true);

    if u.scheme.is_none() {
        u.scheme = b.scheme.clone();
    }
    if u.scheme != b.scheme
        || (u
            .scheme
            .as_deref()
            .is_some_and(|s| !USES_RELATIVE.contains(&s)))
    {
        // _coerce_result(url): the processed (stripped) url
        return u.processed;
    }
    if u.scheme.as_deref().is_none_or(|s| USES_NETLOC.contains(&s)) {
        if u.netloc.as_ref().is_some_and(|n| !n.is_empty()) {
            return urlunsplit(
                u.scheme.as_deref(),
                u.netloc.as_deref(),
                &u.path,
                u.query.as_deref(),
                u.fragment.as_deref(),
            );
        }
        u.netloc = b.netloc.clone();
    }

    if u.path.is_empty() {
        let path = b.path.clone();
        if u.query.is_none() {
            let query = b.query.clone();
            if u.fragment.is_none() {
                let fragment = b.fragment.clone();
                return urlunsplit(
                    u.scheme.as_deref(),
                    u.netloc.as_deref(),
                    &path,
                    query.as_deref(),
                    fragment.as_deref(),
                );
            }
            return urlunsplit(
                u.scheme.as_deref(),
                u.netloc.as_deref(),
                &path,
                query.as_deref(),
                u.fragment.as_deref(),
            );
        }
        return urlunsplit(
            u.scheme.as_deref(),
            u.netloc.as_deref(),
            &path,
            u.query.as_deref(),
            u.fragment.as_deref(),
        );
    }

    let mut base_parts: Vec<&str> = b.path.split('/').collect();
    if base_parts.last() != Some(&"") {
        base_parts.pop();
    }

    let segments: Vec<String>;
    if u.path.starts_with('/') {
        segments = u.path.split('/').map(|s| s.to_string()).collect();
    } else {
        // segments = base_parts + path.split('/'); then
        // segments[1:-1] = filter(None, segments[1:-1])
        let path_parts: Vec<String> = u.path.split('/').map(|s| s.to_string()).collect();
        let mut all: Vec<String> = base_parts
            .iter()
            .map(|s| s.to_string())
            .chain(path_parts.iter().cloned())
            .collect();
        let n = all.len();
        if n >= 2 {
            let mut filtered: Vec<String> = Vec::new();
            for s in all.drain(1..n - 1) {
                if !s.is_empty() {
                    filtered.push(s);
                }
            }
            let mut rebuilt: Vec<String> = Vec::with_capacity(filtered.len() + 2);
            rebuilt.push(all.first().cloned().unwrap_or_default());
            rebuilt.extend(filtered);
            rebuilt.push(all.last().cloned().unwrap_or_default());
            all = rebuilt;
        }
        segments = all;
    }

    let mut resolved: Vec<String> = Vec::new();
    for seg in &segments {
        if seg == ".." {
            resolved.pop();
        } else if seg == "." {
            continue;
        } else {
            resolved.push(seg.clone());
        }
    }

    if matches!(segments.last().map(|s| s.as_str()), Some(".") | Some("..")) {
        resolved.push(String::new());
    }

    let joined = resolved.join("/");
    let joined = if joined.is_empty() {
        "/".to_string()
    } else {
        joined
    };
    urlunsplit(
        u.scheme.as_deref(),
        u.netloc.as_deref(),
        &joined,
        u.query.as_deref(),
        u.fragment.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::urljoin;

    fn check(base: &str, url: &str, expected: &str) {
        assert_eq!(
            urljoin(base, url),
            expected,
            "urljoin({:?}, {:?})",
            base,
            url
        );
    }

    #[test]
    fn basic() {
        check("", "a/b", "a/b");
        check("a/b", "", "a/b");
        check("http://x/a/b", "c", "http://x/a/c");
        check("http://x/a/b", "/c", "http://x/c");
        check("http://x/a/b", "https://y/c", "https://y/c");
        check("http://x/a/b", "//y/c", "http://y/c");
        check("http://x/a/b", "c?q=1", "http://x/a/c?q=1");
        check("http://x/a/b", "#frag", "http://x/a/b#frag");
        check("http://x/a/b?q=1", "c", "http://x/a/c");
        check("http://x/a/b?q=1#f", "c#g", "http://x/a/c#g");
        check("http://x/a/", "c", "http://x/a/c");
        check("http://x/a/", "../c", "http://x/c");
        check("http://x/a/b", "../c", "http://x/c");
        check("http://x/a/b", "../../c", "http://x/c");
        check("http://x/a/b", "./c", "http://x/a/c");
        check("http://x/a/b", "c/", "http://x/a/c/");
        check("http://x/a/b", "c/d", "http://x/a/c/d");
        check("http://x", "c", "http://x/c");
        check("http://x/", "c", "http://x/c");
        check("http://x", "/c", "http://x/c");
        check("file:///a/b", "c", "file:///a/c");
        check("file:///a/b", "/c", "file:///c");
        check("file:///a/", "c", "file:///a/c");
        check("a/b", "c", "a/c");
        check("a/b/", "c", "a/b/c");
        check("a", "b", "b");
        check("", "//x/y", "//x/y");
        check("http://x/a/b", "..", "http://x/");
        check("http://x/a/b", ".", "http://x/a/");
        check("http://x/a/b/", "..", "http://x/a/");
        check("http://x/a/b/c", "../d", "http://x/a/d");
        check("http://x/a//b", "c", "http://x/a/c");
        check("http://x", "?q", "http://x?q");
        check("http://x/a", "#f", "http://x/a#f");
        check("http://x/a?b", "#f", "http://x/a?b#f");
        check("http://x/a?b", "c#f", "http://x/c#f");
        check("HTTP://X/a", "c", "http://X/c");
        check("http://x/a b", "c", "http://x/c");
        check("http://x/é", "c", "http://x/c");
        check("mailto:a@b", "c", "c");
        check("http://x/a/b", "c d", "http://x/a/c d");
        check("http://x/a/b", "c%20d", "http://x/a/c%20d");
        check("http://x:8080/a/b", "c", "http://x:8080/a/c");
        check("http://x/a/b", "c;params", "http://x/a/c;params");
        check("http://x/a;v/b", "c", "http://x/a;v/c");
        check("ftp://x/a", "c", "ftp://x/c");
        check("http://x/a/b", "javascript:alert(1)", "javascript:alert(1)");
        check("http://x/a/b", "  c  ", "http://x/a/c  ");
        check("http://x/a/b", "c\t d", "http://x/a/c d");
        check("http://x/a/b", "c\n", "http://x/a/c");
        check("http://x/a/b", "//", "http://x/a/b");
        check("http://x/a/b", "c/..", "http://x/a/");
        check("http://x/a/b", "c/.", "http://x/a/c/");
        check("http://x/a/b", "c/../", "http://x/a/");
        check("http://x/a/b", "c/d/..", "http://x/a/c/");
        check("http://x/a/b", "c/d/.", "http://x/a/c/d/");
        check("http://x/a/b", "../..", "http://x/");
        check("http://x/a/b", "../../..", "http://x/");
        check("http://x/a/b", "c//d", "http://x/a/c/d");
        check("http://x/a/b", "c///d", "http://x/a/c/d");
    }
}
