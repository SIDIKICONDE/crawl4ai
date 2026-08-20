use crawl4ai_html2text::urljoin::urljoin;
use serde_json::Value;

#[test]
fn urljoin_diff() {
    let corpus = std::fs::read_to_string("/tmp/opencode/urljoin_expected.json").unwrap();
    let cases: Vec<Value> = serde_json::from_str(&corpus).unwrap();
    let mut nfail = 0;
    for case in &cases {
        let base = case["base"].as_str().unwrap();
        let url = case["url"].as_str().unwrap();
        let expected = case["result"].as_str().unwrap();
        let got = urljoin(base, url);
        if got != expected {
            nfail += 1;
            println!("MISMATCH urljoin({:?}, {:?})", base, url);
            println!("  py : {:?}", expected);
            println!("  rs : {:?}", got);
        }
    }
    println!("{} urljoin cases, {} mismatches", cases.len(), nfail);
    assert_eq!(nfail, 0);
}
