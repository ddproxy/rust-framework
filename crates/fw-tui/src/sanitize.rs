/// Makes a string from the network safe to draw: control characters and bidi overrides become
/// `?`, so an escape sequence in a peer's name cannot move the cursor or recolour the screen.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            let bidi = matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
            if c.is_control() || bidi {
                '?'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_and_line_breaks_are_neutralised() {
        assert_eq!(printable("ok\u{1b}[2J\nnext"), "ok?[2J?next");
        assert_eq!(printable("bell\u{7}"), "bell?");
        assert_eq!(printable("c1\u{9b}31m"), "c1?31m");
    }

    #[test]
    fn bidi_overrides_are_neutralised() {
        assert_eq!(printable("ab\u{202e}cd"), "ab?cd");
        assert_eq!(printable("x\u{2066}y"), "x?y");
    }

    #[test]
    fn ordinary_text_passes_through_unchanged() {
        assert_eq!(printable("peer-a 12 café ✓"), "peer-a 12 café ✓");
    }
}
