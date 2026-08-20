#![allow(deprecated)]

//! PyO3 bindings: the `crawl4ai_html2text` extension module.
//!
//! `HTML2Text` is a `#[pyclass(dict)]` wrapper around the pure-Rust
//! `Machine` + `Tokenizer`. Tokenizer events are dispatched to the Python
//! `handle_*` methods via `call_method`, so Python subclasses (like
//! `CustomHTML2Text` in the facade) can override them. The Machine never
//! calls back into Python.

pub mod config;
pub mod entities;
pub mod escape;
pub mod state;
pub mod style;
pub mod tables;
pub mod tokenizer;
pub mod urljoin;
pub mod wrap;

use std::sync::Mutex;

use pyo3::exceptions::PyAttributeError;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBool, PyDict, PyString, PyStringMethods};

use crate::state::{Force, Machine};
use crate::tables::pad_tables_in_text;
use crate::tokenizer::{Event, Tokenizer};

#[pyclass(dict, name = "HTML2Text")]
pub struct HTML2Text {
    machine: Mutex<Machine>,
    tokenizer: Mutex<Tokenizer>,
    out_callback: Option<PyObject>,
    tag_callback: Option<PyObject>,
}

fn attrs_to_pydict<'py>(
    py: Python<'py>,
    attrs: &[(String, Option<String>)],
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, v) in attrs {
        match v {
            Some(v) => d.set_item(k, v)?,
            None => d.set_item(k, py.None())?,
        }
    }
    Ok(d)
}

fn attrs_from_pydict(attrs: &Bound<'_, PyAny>) -> PyResult<Vec<(String, Option<String>)>> {
    let mut out = Vec::new();
    if let Ok(d) = attrs.downcast::<PyDict>() {
        for (k, v) in d.iter() {
            let k: String = k.extract()?;
            let v: Option<String> = v.extract()?;
            out.push((k, v));
        }
    }
    Ok(out)
}

fn props_from_pydict(props: &Bound<'_, PyAny>) -> PyResult<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Ok(d) = props.downcast::<PyDict>() {
        for (k, v) in d.iter() {
            let k: String = k.extract()?;
            let v: String = v.extract()?;
            out.push((k, v));
        }
    }
    Ok(out)
}

impl HTML2Text {
    fn run_events(
        &self,
        py: Python<'_>,
        slf: &Bound<'_, Self>,
        events: Vec<Event>,
    ) -> PyResult<()> {
        for event in events {
            match event {
                Event::Data(d) => {
                    slf.call_method1("handle_data", (d,))?;
                }
                Event::StartTag(tag, attrs) => {
                    let dict = attrs_to_pydict(py, &attrs)?;
                    slf.call_method1("handle_starttag", (tag, dict))?;
                }
                Event::EndTag(tag) => {
                    slf.call_method1("handle_endtag", (tag,))?;
                }
                Event::CharRef(c) => {
                    slf.call_method1("handle_charref", (c,))?;
                }
                Event::EntityRef(c) => {
                    slf.call_method1("handle_entityref", (c,))?;
                }
                Event::StartEndTag(tag, attrs) => {
                    let dict = attrs_to_pydict(py, &attrs)?;
                    slf.call_method1("handle_startendtag", (tag, dict))?;
                }
                Event::Comment(d) => {
                    slf.call_method1("handle_comment", (d,))?;
                }
                Event::Pi(d) => {
                    slf.call_method1("handle_pi", (d,))?;
                }
                Event::Decl(d) => {
                    slf.call_method1("handle_decl", (d,))?;
                }
                Event::Cdata(d) => {
                    slf.call_method1("handle_data", (d,))?;
                }
                Event::UnknownDecl(d) => {
                    slf.call_method1("unknown_decl", (d,))?;
                }
            }
            self.flush_out(py)?;
        }
        Ok(())
    }

    /// External sink: drain pending_out into the custom `out` callback.
    fn flush_out(&self, py: Python<'_>) -> PyResult<()> {
        let cb = match &self.out_callback {
            Some(c) => c.clone_ref(py),
            None => return Ok(()),
        };
        let chunks: Vec<String> = self.machine.lock().unwrap().pending_out.drain(..).collect();
        for c in chunks {
            cb.call1(py, (c,))?;
        }
        Ok(())
    }
}

#[pymethods]
impl HTML2Text {
    #[new]
    #[pyo3(signature = (out=None, baseurl="", bodywidth=None))]
    fn new(
        out: Option<PyObject>,
        baseurl: &str,
        bodywidth: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut machine = Machine::new();
        machine.set_baseurl(baseurl);
        match bodywidth {
            // explicit None -> no wrapping (html2text() passes it)
            Some(b) if b.is_none() => machine.body_width = 0,
            Some(b) => machine.body_width = b.extract()?,
            // omitted -> config default (78)
            None => {}
        }
        let (out_callback, sink_internal) = match out {
            Some(obj) => (Some(obj), false),
            None => (None, true),
        };
        machine.sink_internal = sink_internal;
        Ok(HTML2Text {
            machine: Mutex::new(machine),
            tokenizer: Mutex::new(Tokenizer::new()),
            out_callback,
            tag_callback: None,
        })
    }

    fn __getattr__(slf: &Bound<'_, Self>, name: &str) -> PyResult<PyObject> {
        let py = slf.py();
        let h = slf.borrow();
        let m = h.machine.lock().unwrap();
        let val: PyObject = match name {
            // strings
            "ul_item_mark" => m.ul_item_mark.clone().into_py(py),
            "emphasis_mark" => m.emphasis_mark.clone().into_py(py),
            "strong_mark" => m.strong_mark.clone().into_py(py),
            "br_toggle" => m.br_toggle.clone().into_py(py),
            "baseurl" => m.baseurl.clone().into_py(py),
            "default_image_alt" => m.default_image_alt.clone().into_py(py),
            "open_quote" => m.open_quote.clone().into_py(py),
            "close_quote" => m.close_quote.clone().into_py(py),
            "current_tag" => m.current_tag.clone().into_py(py),
            "preceding_data" => m.preceding_data.clone().into_py(py),
            // bools
            "split_next_td" => m.split_next_td.into_py(py),
            "table_start" => m.table_start.into_py(py),
            "unicode_snob" => m.unicode_snob.into_py(py),
            "escape_snob" => m.escape_snob.into_py(py),
            "escape_backslash" => m.escape_backslash.into_py(py),
            "escape_dot" => m.escape_dot.into_py(py),
            "escape_plus" => m.escape_plus.into_py(py),
            "escape_dash" => m.escape_dash.into_py(py),
            "links_each_paragraph" => m.links_each_paragraph.into_py(py),
            "skip_internal_links" => m.skip_internal_links.into_py(py),
            "inline_links" => m.inline_links.into_py(py),
            "protect_links" => m.protect_links.into_py(py),
            "ignore_links" => m.ignore_links.into_py(py),
            "ignore_mailto_links" => m.ignore_mailto_links.into_py(py),
            "ignore_images" => m.ignore_images.into_py(py),
            "images_as_html" => m.images_as_html.into_py(py),
            "images_to_alt" => m.images_to_alt.into_py(py),
            "images_with_size" => m.images_with_size.into_py(py),
            "ignore_emphasis" => m.ignore_emphasis.into_py(py),
            "bypass_tables" => m.bypass_tables.into_py(py),
            "ignore_tables" => m.ignore_tables.into_py(py),
            "google_doc" => m.google_doc.into_py(py),
            "single_line_break" => m.single_line_break.into_py(py),
            "use_automatic_links" => m.use_automatic_links.into_py(py),
            "hide_strikethrough" => m.hide_strikethrough.into_py(py),
            "mark_code" => m.mark_code.into_py(py),
            "wrap_list_items" => m.wrap_list_items.into_py(py),
            "wrap_links" => m.wrap_links.into_py(py),
            "wrap_tables" => m.wrap_tables.into_py(py),
            "pad_tables" => m.pad_tables.into_py(py),
            "include_sup_sub" => m.include_sup_sub.into_py(py),
            "start" => m.start.into_py(py),
            "space" => m.space.into_py(py),
            "empty_link" => m.empty_link.into_py(py),
            "pre" => m.pre.into_py(py),
            "startpre" => m.startpre.into_py(py),
            "code" => m.code.into_py(py),
            "quote" => m.quote.into_py(py),
            "lastWasNL" => m.last_was_nl.into_py(py),
            "lastWasList" => m.last_was_list.into_py(py),
            "inheader" => m.inheader.into_py(py),
            "stressed" => m.stressed.into_py(py),
            "preceding_stressed" => m.preceding_stressed.into_py(py),
            "inside_link" => m.inside_link.into_py(py),
            // ints
            "td_count" => m.td_count.into_py(py),
            "body_width" => m.body_width.into_py(py),
            "google_list_indent" => m.google_list_indent.into_py(py),
            "quiet" => m.quiet.into_py(py),
            "p_p" => m.p_p.into_py(py),
            "outcount" => m.outcount.into_py(py),
            "acount" => m.acount.into_py(py),
            "blockquote" => m.blockquote.into_py(py),
            "style" => m.style.into_py(py),
            "emphasis" => m.emphasis.into_py(py),
            "drop_white_space" => m.drop_white_space.into_py(py),
            // Optional[str]
            "maybe_automatic_link" => m.maybe_automatic_link.clone().into_py(py),
            "abbr_title" => m.abbr_title.clone().into_py(py),
            "abbr_data" => m.abbr_data.clone().into_py(py),
            // out: the sink (callback or bound outtextf)
            "out" => match slf.borrow().out_callback.as_ref() {
                Some(cb) => cb.clone_ref(py).into_py(py),
                None => slf.getattr("outtextf")?.unbind(),
            },
            "tag_callback" => match slf.borrow().tag_callback.as_ref() {
                Some(cb) => cb.clone_ref(py).into_py(py),
                None => py.None(),
            },
            _ => {
                return Err(PyAttributeError::new_err(format!(
                    "'HTML2Text' object has no attribute '{}'",
                    name
                )))
            }
        };
        Ok(val)
    }

    fn __setattr__(
        slf: &Bound<'_, Self>,
        name: &Bound<'_, PyString>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let name = name.to_string_lossy().into_owned();
        match name.as_str() {
            "out" => {
                if value.is_none() {
                    slf.borrow_mut().out_callback = None;
                    slf.borrow().machine.lock().unwrap().sink_internal = true;
                } else {
                    slf.borrow_mut().out_callback = Some(value.clone().unbind());
                    slf.borrow().machine.lock().unwrap().sink_internal = false;
                }
            }
            "tag_callback" => {
                if value.is_none() {
                    slf.borrow_mut().tag_callback = None;
                } else {
                    slf.borrow_mut().tag_callback = Some(value.clone().unbind());
                }
            }
            _ => {
                let slf_ref = slf.borrow();
                let mut m = slf_ref.machine.lock().unwrap();
                match name.as_str() {
            "ul_item_mark" => m.ul_item_mark = value.extract()?,
            "emphasis_mark" => m.emphasis_mark = value.extract()?,
            "strong_mark" => m.strong_mark = value.extract()?,
            "br_toggle" => m.br_toggle = value.extract()?,
            "baseurl" => m.baseurl = value.extract()?,
            "default_image_alt" => m.default_image_alt = value.extract()?,
            "open_quote" => m.open_quote = value.extract()?,
            "close_quote" => m.close_quote = value.extract()?,
            "current_tag" => m.current_tag = value.extract()?,
            "preceding_data" => m.preceding_data = value.extract()?,
            "split_next_td" => m.split_next_td = value.extract()?,
            "table_start" => m.table_start = value.extract()?,
            "unicode_snob" => m.unicode_snob = value.extract()?,
            "escape_snob" => m.escape_snob = value.extract()?,
            "escape_backslash" => m.escape_backslash = value.extract()?,
            "escape_dot" => m.escape_dot = value.extract()?,
            "escape_plus" => m.escape_plus = value.extract()?,
            "escape_dash" => m.escape_dash = value.extract()?,
            "links_each_paragraph" => m.links_each_paragraph = value.extract()?,
            "skip_internal_links" => m.skip_internal_links = value.extract()?,
            "inline_links" => m.inline_links = value.extract()?,
            "protect_links" => m.protect_links = value.extract()?,
            "ignore_links" => m.ignore_links = value.extract()?,
            "ignore_mailto_links" => m.ignore_mailto_links = value.extract()?,
            "ignore_images" => m.ignore_images = value.extract()?,
            "images_as_html" => m.images_as_html = value.extract()?,
            "images_to_alt" => m.images_to_alt = value.extract()?,
            "images_with_size" => m.images_with_size = value.extract()?,
            "ignore_emphasis" => m.ignore_emphasis = value.extract()?,
            "bypass_tables" => m.bypass_tables = value.extract()?,
            "ignore_tables" => m.ignore_tables = value.extract()?,
            "google_doc" => m.google_doc = value.extract()?,
            "single_line_break" => m.single_line_break = value.extract()?,
            "use_automatic_links" => m.use_automatic_links = value.extract()?,
            "hide_strikethrough" => m.hide_strikethrough = value.extract()?,
            "mark_code" => m.mark_code = value.extract()?,
            "wrap_list_items" => m.wrap_list_items = value.extract()?,
            "wrap_links" => m.wrap_links = value.extract()?,
            "wrap_tables" => m.wrap_tables = value.extract()?,
            "pad_tables" => m.pad_tables = value.extract()?,
            "include_sup_sub" => m.include_sup_sub = value.extract()?,
            "start" => m.start = value.extract()?,
            "space" => m.space = value.extract()?,
            "empty_link" => m.empty_link = value.extract()?,
            "pre" => m.pre = value.extract()?,
            "startpre" => m.startpre = value.extract()?,
            "code" => m.code = value.extract()?,
            "quote" => m.quote = value.extract()?,
            "lastWasNL" => m.last_was_nl = value.extract()?,
            "lastWasList" => m.last_was_list = value.extract()?,
            "inheader" => m.inheader = value.extract()?,
            "stressed" => m.stressed = value.extract()?,
            "preceding_stressed" => m.preceding_stressed = value.extract()?,
            "inside_link" => m.inside_link = value.extract()?,
            "td_count" => m.td_count = value.extract()?,
            "body_width" => m.body_width = value.extract()?,
            "google_list_indent" => m.google_list_indent = value.extract()?,
            "quiet" => m.quiet = value.extract()?,
            "p_p" => m.p_p = value.extract()?,
            "outcount" => m.outcount = value.extract()?,
            "acount" => m.acount = value.extract()?,
            "blockquote" => m.blockquote = value.extract()?,
            "style" => m.style = value.extract()?,
            "emphasis" => m.emphasis = value.extract()?,
            "drop_white_space" => m.drop_white_space = value.extract()?,
            "maybe_automatic_link" => m.maybe_automatic_link = value.extract()?,
            "abbr_title" => m.abbr_title = value.extract()?,
            "abbr_data" => m.abbr_data = value.extract()?,
            "outtextlist" => m.outtextlist = value.extract()?,
                    _ => {
                        drop(m);
                        let dict = slf.getattr("__dict__")?.downcast_into::<PyDict>()?;
                        dict.set_item(name.as_str(), value)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn update_params(slf: &Bound<'_, Self>, kwargs: &Bound<'_, PyDict>) -> PyResult<()> {
        for (k, v) in kwargs.iter() {
            let k: String = k.extract()?;
            slf.setattr(k.as_str(), v)?;
        }
        Ok(())
    }

    fn set_baseurl(&self, url: &str) {
        self.machine.lock().unwrap().set_baseurl(url);
    }

    fn feed(slf: &Bound<'_, Self>, data: &str) -> PyResult<()> {
        let data = data.replace("</' + 'script>", "</ignore>");
        let mut events = Vec::new();
        {
            let h = slf.borrow();
            let mut tok = h.tokenizer.lock().unwrap();
            tok.feed(&data, &mut events);
        }
        slf.borrow().run_events(slf.py(), slf, events)
    }

    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        let mut events = Vec::new();
        {
            let h = slf.borrow();
            let mut tok = h.tokenizer.lock().unwrap();
            tok.close(&mut events);
        }
        slf.borrow().run_events(slf.py(), slf, events)
    }

    fn finish(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.call_method0("close")?;
        {
            let h = slf.borrow();
            let mut m = h.machine.lock().unwrap();
            m.pbr();
            m.o("", false, Force::End);
        }
        let out = slf.borrow().machine.lock().unwrap().finish_text();
        slf.borrow().flush_out(slf.py())?;
        Ok(out)
    }

    fn handle(slf: &Bound<'_, Self>, data: &str) -> PyResult<String> {
        slf.borrow().machine.lock().unwrap().start = true;
        slf.call_method1("feed", (data,))?;
        slf.call_method1("feed", ("",))?;
        let finished = slf.call_method0("finish")?.extract::<String>()?;
        let markdown = slf.borrow().machine.lock().unwrap().optwrap(&finished);
        if slf.borrow().machine.lock().unwrap().pad_tables {
            Ok(pad_tables_in_text(&markdown, 1))
        } else {
            Ok(markdown)
        }
    }

    fn outtextf(&self, s: &str) {
        self.machine.lock().unwrap().outtextf(s);
    }

    #[pyo3(signature = (data, puredata=false, force=None))]
    fn o(&self, data: &str, puredata: bool, force: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let f = match force {
            None => Force::None,
            Some(f) => {
                if f.extract::<String>().map(|s| s == "end").unwrap_or(false) {
                    Force::End
                } else if f.is_truthy()? {
                    Force::Truthy
                } else {
                    Force::None
                }
            }
        };
        self.machine.lock().unwrap().o(data, puredata, f);
        Ok(())
    }

    fn p(&self) {
        self.machine.lock().unwrap().p();
    }

    fn pbr(&self) {
        self.machine.lock().unwrap().pbr();
    }

    fn soft_br(&self) {
        self.machine.lock().unwrap().soft_br();
    }

    fn optwrap(&self, text: &str) -> String {
        self.machine.lock().unwrap().optwrap(text)
    }

    #[pyo3(signature = (data, entity_char=false))]
    fn handle_data(&self, data: &str, entity_char: bool) {
        self.machine.lock().unwrap().handle_data(data, entity_char);
    }

    fn handle_tag(
        slf: &Bound<'_, Self>,
        tag: &str,
        attrs: &Bound<'_, PyAny>,
        start: bool,
    ) -> PyResult<()> {
        let attrs_vec = attrs_from_pydict(attrs)?;
        {
            let h = slf.borrow();
            let mut m = h.machine.lock().unwrap();
            m.current_tag = tag.to_string();
        }
        if let Some(cb) = slf.borrow().tag_callback.as_ref() {
            let r = cb.call1(slf.py(), (slf, tag, attrs, start))?;
            let truth = PyBool::new(slf.py(), true);
            if r.is(truth.as_any()) {
                return Ok(());
            }
        }
        slf.borrow()
            .machine
            .lock()
            .unwrap()
            .handle_tag(tag, attrs_vec, start);
        Ok(())
    }

    fn handle_charref(slf: &Bound<'_, Self>, c: &str) -> PyResult<()> {
        let s = slf.borrow().machine.lock().unwrap().charref(c);
        slf.call_method1("handle_data", (s, true))?;
        Ok(())
    }

    fn handle_entityref(slf: &Bound<'_, Self>, c: &str) -> PyResult<()> {
        let s = slf.borrow().machine.lock().unwrap().entityref(c);
        if !s.is_empty() {
            slf.call_method1("handle_data", (s, true))?;
        }
        Ok(())
    }

    fn handle_starttag(slf: &Bound<'_, Self>, tag: &str, attrs: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        // Python does dict(attrs); the tokenizer already hands us a dict.
        let dict = if attrs.downcast::<PyDict>().is_ok() {
            attrs.to_owned()
        } else {
            let vec = attrs_from_pydict(attrs)?;
            attrs_to_pydict(py, &vec)?.into_any()
        };
        slf.call_method1("handle_tag", (tag, dict, true))?;
        Ok(())
    }

    fn handle_endtag(slf: &Bound<'_, Self>, tag: &str) -> PyResult<()> {
        let d = PyDict::new(slf.py());
        slf.call_method1("handle_tag", (tag, d, false))?;
        Ok(())
    }

    fn handle_startendtag(
        slf: &Bound<'_, Self>,
        tag: &str,
        attrs: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        slf.call_method1("handle_starttag", (tag, attrs))?;
        slf.call_method1("handle_endtag", (tag,))?;
        Ok(())
    }

    fn handle_comment(&self, _data: &str) {}

    fn handle_pi(&self, _data: &str) {}

    fn handle_decl(&self, _decl: &str) {}

    fn unknown_decl(&self, _data: &str) {}

    fn charref(&self, name: &str) -> String {
        self.machine.lock().unwrap().charref(name)
    }

    fn entityref(&self, c: &str) -> String {
        self.machine.lock().unwrap().entityref(c)
    }

    #[pyo3(name = "previousIndex")]
    fn previous_index(&self, attrs: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        let attrs_vec = attrs_from_pydict(attrs)?;
        Ok(self.machine.lock().unwrap().previous_index(&attrs_vec))
    }

    fn google_nest_count(&self, style: &Bound<'_, PyAny>) -> PyResult<usize> {
        let style_vec = props_from_pydict(style)?;
        Ok(self.machine.lock().unwrap().google_nest_count(&style_vec))
    }

    fn handle_emphasis(
        &self,
        start: bool,
        tag_style: &Bound<'_, PyAny>,
        parent_style: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let tag_style = props_from_pydict(tag_style)?;
        let parent_style = props_from_pydict(parent_style)?;
        self.machine
            .lock()
            .unwrap()
            .handle_emphasis(start, &tag_style, &parent_style);
        Ok(())
    }

    fn hn(&self, tag: &str) -> usize {
        crate::style::hn(tag)
    }
}

#[pyfunction]
#[pyo3(signature = (html, baseurl="", bodywidth=None))]
fn html2text(
    py: Python<'_>,
    html: &str,
    baseurl: &str,
    bodywidth: Option<usize>,
) -> PyResult<String> {
    let inst = HTML2Text {
        machine: Mutex::new({
            let mut m = Machine::new();
            m.set_baseurl(baseurl);
            m.body_width = bodywidth.unwrap_or(0);
            m
        }),
        tokenizer: Mutex::new(Tokenizer::new()),
        out_callback: None,
        tag_callback: None,
    };
    let h = Py::new(py, inst)?;
    let out = h.bind(py).call_method1("handle", (html,))?;
    out.extract()
}

#[pymodule]
fn crawl4ai_html2text(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<HTML2Text>()?;
    m.add_function(wrap_pyfunction!(html2text, m)?)?;
    Ok(())
}
