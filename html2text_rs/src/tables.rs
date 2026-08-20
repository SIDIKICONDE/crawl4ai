//! Table padding helpers mirroring crawl4ai/html2text/utils.py
//! (reformat_table, pad_tables_in_text)

use crate::config::TABLE_MARKER_FOR_PAD;

/// Given the lines of a table, pads the cells and returns the new lines.
fn reformat_table(lines: &[String], right_margin: usize) -> Vec<String> {
    let mut max_width: Vec<usize> = lines[0]
        .split('|')
        .map(|x| x.trim_end().len() + right_margin)
        .collect();
    let mut max_cols = max_width.len();
    for line in lines {
        let cols: Vec<&str> = line.split('|').collect();
        let num_cols = cols.len();

        // don't drop any data if colspan attributes result in unequal lengths
        if num_cols < max_cols {
            // pad with empty cells
        } else if max_cols < num_cols {
            for x in cols[max_cols..].iter() {
                max_width.push(x.trim_end().len() + right_margin);
            }
            max_cols = num_cols;
        }

        for (i, x) in cols.iter().enumerate() {
            if i < max_width.len() {
                let w = x.trim_end().len() + right_margin;
                if w > max_width[i] {
                    max_width[i] = w;
                }
            }
        }
    }

    let mut new_lines = Vec::new();
    for line in lines {
        let cols: Vec<&str> = line.split('|').collect();
        // Python: set(line.strip()) == set("-|")  -> exactly dashes and pipes
        let trimmed = line.trim();
        let is_separator = !trimmed.is_empty()
            && trimmed.chars().all(|c| c == '-' || c == '|')
            && trimmed.contains('-')
            && trimmed.contains('|');
        if is_separator {
            let mut new_cols: Vec<String> = Vec::new();
            for (x, m) in cols.iter().zip(max_width.iter()) {
                let t = x.trim_end();
                let pad = "-".repeat(m.saturating_sub(t.len()));
                new_cols.push(format!("{}{}", t, pad));
            }
            new_lines.push(format!("|-{}|", new_cols.join("|")));
        } else {
            let mut new_cols: Vec<String> = Vec::new();
            for (x, m) in cols.iter().zip(max_width.iter()) {
                let t = x.trim_end();
                let pad = " ".repeat(m.saturating_sub(t.len()));
                new_cols.push(format!("{}{}", t, pad));
            }
            new_lines.push(format!("| {}|", new_cols.join("|")));
        }
    }
    new_lines
}

/// Provide padding for tables in the text.
pub fn pad_tables_in_text(text: &str, right_margin: usize) -> String {
    let mut table_buffer: Vec<String> = Vec::new();
    let mut table_started = false;
    let mut new_lines: Vec<String> = Vec::new();

    for line in text.split('\n') {
        if line.contains(TABLE_MARKER_FOR_PAD) {
            table_started = !table_started;
            if !table_started {
                let table = reformat_table(&table_buffer, right_margin);
                new_lines.extend(table);
                table_buffer.clear();
                new_lines.push(String::new());
            }
            continue;
        }
        if table_started {
            table_buffer.push(line.to_string());
        } else {
            new_lines.push(line.to_string());
        }
    }
    new_lines.join("\n")
}
