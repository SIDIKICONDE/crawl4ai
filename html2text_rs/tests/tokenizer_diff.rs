use crawl4ai_html2text::tokenizer::{Event, Tokenizer};
use serde_json::{json, Value};

fn ev_to_json(e: &Event) -> Value {
    match e {
        Event::Data(s) => json!(["data", s]),
        Event::StartTag(t, a) => json!(["start", t, attr_json(a)]),
        Event::EndTag(t) => json!(["end", t]),
        Event::StartEndTag(t, a) => json!(["startend", t, attr_json(a)]),
        Event::CharRef(s) => json!(["charref", s]),
        Event::EntityRef(s) => json!(["entityref", s]),
        Event::Comment(s) => json!(["comment", s]),
        Event::Pi(s) => json!(["pi", s]),
        Event::Decl(s) => json!(["decl", s]),
        Event::Cdata(s) => json!(["cdata", s]),
        Event::UnknownDecl(s) => json!(["unknown_decl", s]),
    }
}

fn attr_json(a: &[(String, Option<String>)]) -> Value {
    Value::Array(a.iter().map(|(k, v)| json!([k, v])).collect())
}

#[test]
fn tokenizer_diff() {
    let corpus = std::fs::read_to_string("/tmp/opencode/events.json").unwrap();
    let cases: Vec<Value> = serde_json::from_str(&corpus).unwrap();
    let mut nfail = 0;
    for case in &cases {
        let input = case["in"].as_str().unwrap();
        let expected: Vec<Value> = case["events"].as_array().unwrap().clone();

        let mut tok = Tokenizer::new();
        let mut events: Vec<Event> = Vec::new();
        tok.feed(input, &mut events);
        tok.close(&mut events);

        let got: Vec<Value> = events.iter().map(ev_to_json).collect();

        if got != expected {
            nfail += 1;
            println!("MISMATCH input={:?}", input);
            println!("  py : {}", serde_json::to_string(&expected).unwrap());
            println!("  rs : {}", serde_json::to_string(&got).unwrap());
        }
    }
    println!("{} cases, {} mismatches", cases.len(), nfail);
    assert_eq!(nfail, 0);

    // chunked feeds
    let corpus = std::fs::read_to_string("/tmp/opencode/events_chunked.json").unwrap();
    let cases: Vec<Value> = serde_json::from_str(&corpus).unwrap();
    let mut nfail = 0;
    for case in &cases {
        let chunks: Vec<String> = case["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        let expected: Vec<Value> = case["events"].as_array().unwrap().clone();

        let mut tok = Tokenizer::new();
        let mut events: Vec<Event> = Vec::new();
        for c in &chunks {
            tok.feed(c, &mut events);
        }
        tok.close(&mut events);

        let got: Vec<Value> = events.iter().map(ev_to_json).collect();
        if got != expected {
            nfail += 1;
            println!("MISMATCH chunks={:?}", chunks);
            println!("  py : {}", serde_json::to_string(&expected).unwrap());
            println!("  rs : {}", serde_json::to_string(&got).unwrap());
        }
    }
    println!("{} chunked cases, {} mismatches", cases.len(), nfail);
    assert_eq!(nfail, 0);
}
