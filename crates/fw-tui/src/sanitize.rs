/// Makes a string from the network safe to draw: control characters, bidi overrides and
/// invisible format characters (zero-width marks, the byte-order mark) become `?`, so a peer's
/// name cannot move the cursor, recolour the screen, or reorder or hide the text around it.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            let invisible = matches!(
                c,
                '\u{200b}'..='\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2060}'..='\u{2064}'
                    | '\u{2066}'..='\u{206f}'
                    | '\u{feff}'
            );
            if c.is_control() || invisible {
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
    fn zero_width_and_format_characters_are_neutralised() {
        assert_eq!(
            printable("a\u{200b}b\u{feff}c\u{2060}d\u{200f}e"),
            "a?b?c?d?e"
        );
    }

    #[test]
    fn ordinary_text_passes_through_unchanged() {
        assert_eq!(printable("peer-a 12 café ✓"), "peer-a 12 café ✓");
    }
}
