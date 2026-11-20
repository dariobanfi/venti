//! Text that one person types and another reads: what of it is kept.
//!
//! The text of a report, the name of a group, the place and the note of a
//! plan all go through here. None of them is refused for a character in
//! it: what would do harm on the reader's screen is dropped, and what is
//! left is what is stored.

/// Whether a character is dropped from text a pilot wrote: a control
/// character, or one that hides text or turns it round on the reader's
/// screen. A line feed is a control character too; the callers decide what
/// becomes of it before they ask.
fn dropped(c: char) -> bool {
    c.is_control()
        || matches!(
            u32::from(c),
            // Zero-width characters and direction marks.
            0x061C | 0x180E | 0x200B..=0x200F
            // Line and paragraph separators, direction embeddings and
            // overrides.
            | 0x2028..=0x202E
            // Word joiner, invisible operators, direction isolates.
            | 0x2060..=0x206F
            // Byte order mark, interlinear annotation.
            | 0xFEFF | 0xFFF9..=0xFFFB
            // Tag characters.
            | 0xE0000..=0xE007F
        )
}

/// Text of several lines, fit to keep. Line breaks stay, as single line
/// feeds; every other control character goes, and so does what would hide
/// text or turn it round. White space at both ends goes.
pub(crate) fn lines(text: &str) -> String {
    let kept: String = text
        .replace("\r\n", "\n")
        .chars()
        .filter(|c| *c == '\n' || !dropped(*c))
        .collect();
    kept.trim().to_owned()
}

/// Text of one line, fit to keep. A line break, a tab and every other kind
/// of white space become a space, and several in a row become one; what
/// [`lines`] drops is dropped here too. White space at both ends goes.
pub(crate) fn line(text: &str) -> String {
    let spaced: String = text
        .chars()
        // The two separators are white space to Unicode and dropped by
        // `dropped`; here they end a word, as a line feed does.
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !dropped(*c))
        .collect();
    spaced.split(' ').filter(|word| !word.is_empty()).fold(
        String::with_capacity(spaced.len()),
        |mut kept, word| {
            if !kept.is_empty() {
                kept.push(' ');
            }
            kept.push_str(word);
            kept
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_lines_keep_their_line_breaks_and_lose_what_does_harm() {
        assert_eq!(lines("  one\r\ntwo \n"), "one\ntwo");
        assert_eq!(
            lines("a\u{1b}[2Jb\u{0}c\u{7}d\u{202e}e\u{200b}f\u{9b}g\rh\ti\u{2028}j"),
            "a[2Jbcdefghij"
        );
        assert_eq!(lines(" \n\t "), "");
    }

    #[test]
    fn one_line_is_one_line_with_single_spaces() {
        assert_eq!(
            line("  Musterberg,\r\n east\ttake-off  "),
            "Musterberg, east take-off"
        );
        assert_eq!(line("a\u{2028}b\u{a0}c\u{3000}d"), "a b c d");
        assert_eq!(line("a\u{1b}[2Jb\u{0}c\u{202e}d\u{200b}e"), "a[2Jbcde");
        assert_eq!(line(" \n\t\u{200b} "), "");
        // Any script and any sign may stay.
        let words = "Grüße vom «Musterberg» 山 🪂 <b>";
        assert_eq!(line(words), words);
    }
}
