"""html2text: Turn HTML into equivalent Markdown-structured text.

Pure Rust implementation via the `crawl4ai_html2text` PyO3 extension
(sources in `html2text_rs/`). The Python wrapper preserves the
`CustomHTML2Text` subclass for Crawl4AI-specific behaviour (preserved
tags, fenced code blocks for <pre>, <base> handling).
"""

import re as _re
from typing import Optional

from crawl4ai_html2text import HTML2Text as _RustHTML2Text
from crawl4ai_html2text import html2text as _rust_html2text

__version__ = (2026, 1, 0)


# Re-export the Rust HTML2Text as the public class.
HTML2Text = _RustHTML2Text


def html2text(html: str, baseurl: str = "", bodywidth=None) -> str:
    """One-shot HTML to Markdown conversion."""
    return _rust_html2text(html, baseurl, bodywidth)


# Match <base href="..."> so we can apply it to a relative baseurl.
_BASE_HREF_RE = _re.compile(
    r"""<base\b[^>]*?href\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]*))""",
    _re.IGNORECASE,
)


def _extract_base_href(html: str) -> Optional[str]:
    m = _BASE_HREF_RE.search(html)
    if not m:
        return None
    for g in m.groups():
        if g is not None:
            return g
    return None


class CustomHTML2Text:
    """Crawl4AI-flavoured HTML2Text. Wraps the Rust implementation and adds:

    - <pre> blocks emitted as fenced code blocks (```...```).
    - <code> handled as inline backticks (skipped inside <pre>).
    - Optional preservation of selected tags (e.g. math, video) verbatim.
    - <base> tag updates the base URL for relative links.
    - Crawl4AI default option overrides on the underlying Rust instance.

    Cannot inherit the Rust class (PyO3 limitation), so it composes one
    internally. All public methods (handle, handle_tag, handle_data, o,
    p, pbr, soft_br, optwrap, outtextf, feed, close, finish, feed,
    update_params, …) are forwarded to the wrapped Rust instance.
    """

    def __init__(self, *args, handle_code_in_pre=False, **kwargs):
        # Inner Rust instance does the heavy lifting.
        self._inner: _RustHTML2Text = _RustHTML2Text(*args, **kwargs)

        self.inside_pre = False
        self.inside_code = False
        self.inside_link = False
        self.preserve_tags: set = set()
        self.current_preserved_tag = None
        self.preserved_content: list = []
        self.preserve_depth = 0
        self.handle_code_in_pre = handle_code_in_pre

        # Crawl4AI defaults.
        self.skip_internal_links = False
        self.single_line_break = False
        self.mark_code = False
        self.include_sup_sub = False
        self.body_width = 0
        self.ignore_mailto_links = True
        self.ignore_links = False
        self.escape_backslash = False
        self.escape_dot = False
        self.escape_plus = False
        self.escape_dash = False
        self.escape_snob = False

    # ---------- delegated configuration ----------

    def update_params(self, **kwargs):
        """Update parameters and set preserved tags."""
        for key, value in kwargs.items():
            if key == "preserve_tags":
                self.preserve_tags = set(value)
            elif key == "handle_code_in_pre":
                self.handle_code_in_pre = value
            else:
                setattr(self, key, value)
                # Mirror the value onto the inner Rust instance.
                try:
                    setattr(self._inner, key, value)
                except Exception:
                    pass

    def __setattr__(self, name, value):
        super().__setattr__(name, value)
        # Anything that is also a Rust attribute: mirror it.
        if name not in {
            "_inner",
            "inside_pre",
            "inside_code",
            "inside_link",
            "preserve_tags",
            "current_preserved_tag",
            "preserved_content",
            "preserve_depth",
            "handle_code_in_pre",
        }:
            try:
                inner = self.__dict__.get("_inner")
                if inner is not None and hasattr(inner, name):
                    setattr(inner, name, value)
            except Exception:
                pass

    # ---------- tag & data hooks (called by the inner instance) ----------

    def handle_tag(self, tag, attrs, start):
        # <base> tag: update base URL even when emitted from <head>.
        if tag == "base" and start:
            href = attrs.get("href") if attrs else None
            if href:
                self._inner.baseurl = href
            return None

        # Preserved tags: write back verbatim.
        if tag in self.preserve_tags:
            if start:
                if self.preserve_depth == 0:
                    self.current_preserved_tag = tag
                    self.preserved_content = []
                    attr_str = "".join(
                        f' {k}="{v}"' for k, v in attrs.items() if v is not None
                    )
                    self.preserved_content.append(f"<{tag}{attr_str}>")
                self.preserve_depth += 1
                return None
            else:
                self.preserve_depth -= 1
                if self.preserve_depth == 0:
                    self.preserved_content.append(f"</{tag}>")
                    preserved_html = "".join(self.preserved_content)
                    self._inner.o("\n" + preserved_html + "\n")
                    self.current_preserved_tag = None
                return None

        if self.preserve_depth > 0:
            if start:
                attr_str = "".join(
                    f' {k}="{v}"' for k, v in attrs.items() if v is not None
                )
                self.preserved_content.append(f"<{tag}{attr_str}>")
            else:
                self.preserved_content.append(f"</{tag}>")
            return None

        # <pre> → fenced code block.
        if tag == "pre":
            if start:
                lang = attrs.get("data-language", "") if attrs else ""
                self._inner.o(f"\n```{lang}\n")
                self.inside_pre = True
            else:
                self._inner.o("\n```\n")
                self.inside_pre = False
            return None

        if tag == "code":
            if self.inside_pre and not self.handle_code_in_pre:
                return None
            if start:
                if not self.inside_link:
                    self._inner.o("`")
                self.inside_code = True
            else:
                if not self.inside_link:
                    self._inner.o("`")
                self.inside_code = False
            # Let Rust continue with normal tag handling if inside a link.
            if self.inside_link:
                return self._inner.handle_tag(tag, attrs, start)
            return None

        return None  # let Rust continue

    def handle_data(self, data, entity_char=False):
        if self.preserve_depth > 0:
            self.preserved_content.append(data)
            return None
        if self.inside_pre:
            self._inner.o(data)
            return None
        if self.inside_code:
            self._inner.o(data.replace("\n", " "))
            return None
        return None  # let Rust continue

    # ---------- main entry point ----------

    def handle(self, html: str) -> str:
        # Reset per-run state.
        self.inside_pre = False
        self.inside_code = False
        self.preserve_depth = 0
        self.preserved_content = []
        self.current_preserved_tag = None

        # <base> tag handling: apply to the inner instance's baseurl.
        base_href = _extract_base_href(html)
        if base_href:
            self._inner.baseurl = base_href

        # Wire the Python hooks into the inner instance so the tokenizer
        # dispatch (which lives in Rust) can call them.
        self._inner.tag_callback = self._make_tag_callback()

        try:
            return self._inner.handle(html)
        finally:
            self._inner.tag_callback = None

    def _make_tag_callback(self):
        """Build a callable the Rust Machine invokes before default tag
        handling. Returns True to suppress Rust's default handling."""

        def callback(_self, tag, attrs, start):
            attrs_dict = dict(attrs) if attrs else {}
            result = self.handle_tag(tag, attrs_dict, start)
            return result is None  # None → keep going; True → handled.

        return callback

    # ---------- delegated proxy methods ----------

    def feed(self, data: str) -> None:
        return self._inner.feed(data)

    def close(self) -> None:
        return self._inner.close()

    def finish(self) -> str:
        return self._inner.finish()

    def outtextf(self, s: str) -> None:
        return self._inner.outtextf(s)

    def o(self, data, puredata=False, force=None) -> None:
        return self._inner.o(data, puredata, force)

    def p(self) -> None:
        return self._inner.p()

    def pbr(self) -> None:
        return self._inner.pbr()

    def soft_br(self) -> None:
        return self._inner.soft_br()

    def optwrap(self, text: str) -> str:
        return self._inner.optwrap(text)

    def charref(self, name: str) -> str:
        return self._inner.charref(name)

    def entityref(self, c: str) -> str:
        return self._inner.entityref(c)

    def set_baseurl(self, url: str) -> None:
        return self._inner.set_baseurl(url)

    def previousIndex(self, attrs):
        return self._inner.previousIndex(attrs)

    def google_nest_count(self, style):
        return self._inner.google_nest_count(style)
