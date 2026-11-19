/// Why a line from an instrument was thrown away.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SentenceError {
    /// The checksum at the end of the line does not match its contents, so
    /// the line was damaged on the way.
    #[error("the sentence's checksum does not match")]
    Checksum,
    /// The line is not text.
    #[error("the sentence is not text")]
    NotText,
}

/// The longest line kept while waiting for its end. Real sentences are under
/// a hundred characters; anything longer is noise.
const MAX_LINE: usize = 256;

/// Cuts a byte stream into sentences.
///
/// Feed it bytes as they arrive with [`push`](Self::push); it returns the
/// complete lines found so far and keeps the unfinished rest for next time.
#[derive(Debug, Default)]
pub struct SentenceReader {
    pending: Vec<u8>,
}

impl SentenceReader {
    /// A reader with nothing buffered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds bytes and returns every line they complete, in order.
    ///
    /// A line that starts with `$` and ends in `*hh` has its checksum
    /// verified and stripped: the two hexadecimal digits must equal the
    /// exclusive-or of every character between the `$` and the `*`. Lines
    /// without a checksum are passed through, because some instruments send
    /// none.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<String, SentenceError>> {
        let mut lines = Vec::new();
        for &byte in bytes {
            if byte == b'\n' || byte == b'\r' {
                if !self.pending.is_empty() {
                    lines.push(finish(&std::mem::take(&mut self.pending)));
                }
            } else if self.pending.len() < MAX_LINE {
                self.pending.push(byte);
            } else {
                // Overlong: drop it and resynchronise on the next line end.
                self.pending.clear();
            }
        }
        lines
    }
}

fn finish(line: &[u8]) -> Result<String, SentenceError> {
    let text = std::str::from_utf8(line)
        .map_err(|_| SentenceError::NotText)?
        .trim();
    let Some(body) = text.strip_prefix('$') else {
        return Ok(text.to_owned());
    };
    let Some((content, checksum)) = body.rsplit_once('*') else {
        return Ok(text.to_owned());
    };
    let expected = u8::from_str_radix(checksum.trim(), 16).map_err(|_| SentenceError::Checksum)?;
    let actual = content.bytes().fold(0, |sum, byte| sum ^ byte);
    if actual == expected {
        Ok(format!("${content}"))
    } else {
        Err(SentenceError::Checksum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sentence_split_across_deliveries_is_put_back_together() {
        let mut reader = SentenceReader::new();
        assert!(reader.push(b"$LXWP0,N,,119.9,0.1").is_empty());
        assert_eq!(
            reader.push(b"6,,,,,,259,,*64\r\n$LX"),
            [Ok("$LXWP0,N,,119.9,0.16,,,,,,259,,".to_owned())]
        );
    }

    #[test]
    fn several_sentences_in_one_delivery_come_out_in_order() {
        let mut reader = SentenceReader::new();
        let lines = reader.push(b"PRS 17CBA\nBAT 1234\r\n\r\n");
        assert_eq!(
            lines,
            [Ok("PRS 17CBA".to_owned()), Ok("BAT 1234".to_owned())]
        );
    }

    #[test]
    fn a_damaged_sentence_is_refused() {
        let mut reader = SentenceReader::new();
        let lines = reader.push(b"$LXWP0,N,,119.9,9.16,,,,,,259,,*64\n");
        assert_eq!(lines, [Err(SentenceError::Checksum)]);
        let lines = reader.push(b"$LXWP0,N,,119.9,0.16,,,,,,259,,*zz\n");
        assert_eq!(lines, [Err(SentenceError::Checksum)]);
    }

    #[test]
    fn a_sentence_without_a_checksum_is_passed_through() {
        let mut reader = SentenceReader::new();
        assert_eq!(
            reader.push(b"$POV,P,1013.25\n"),
            [Ok("$POV,P,1013.25".to_owned())]
        );
    }

    #[test]
    fn noise_without_a_line_end_does_not_grow_without_limit() {
        let mut reader = SentenceReader::new();
        assert!(reader.push(&[b'x'; 10_000]).is_empty());
        assert!(reader.pending.len() <= MAX_LINE);
        // The reader recovers at the next line end.
        reader.push(b"\n");
        assert_eq!(reader.push(b"PRS 17CBA\n"), [Ok("PRS 17CBA".to_owned())]);
    }

    #[test]
    fn bytes_that_are_not_text_are_refused() {
        let mut reader = SentenceReader::new();
        assert_eq!(
            reader.push(&[0xff, 0xfe, b'\n']),
            [Err(SentenceError::NotText)]
        );
    }
}
