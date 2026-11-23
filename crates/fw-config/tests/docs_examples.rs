//! Parses every TOML example in the configuration reference with the real validator.

use std::fs;

use fw_config::Config;

const DOC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/configuration.md");

fn fenced_examples(text: &str, tag: &str) -> Vec<String> {
    let fence = format!("```{tag}");
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if line.trim_end() == "```" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
        } else if line.trim_end() == fence.as_str() {
            current = Some(String::new());
        } else if let Some(block) = current.as_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks
}

#[test]
fn configuration_examples_parse() {
    let text = fs::read_to_string(DOC).expect("docs/configuration.md should be readable");
    let examples = fenced_examples(&text, "toml");
    assert!(!examples.is_empty(), "no TOML examples found in the docs");
    for (index, example) in examples.iter().enumerate() {
        Config::parse(example)
            .unwrap_or_else(|error| panic!("example {} failed: {error}", index + 1));
    }
}
