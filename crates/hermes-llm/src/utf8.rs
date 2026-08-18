//! Carry incomplete UTF-8 across network chunks.
//!
//! SSE streams split on arbitrary byte boundaries. A Chinese character is
//! three UTF-8 bytes; decoding a split chunk with `from_utf8_lossy` turns
//! it into U+FFFD (`�`), which the user sees as `���`.

#[derive(Default)]
pub struct Utf8Carry {
    pending: Vec<u8>,
}

impl Utf8Carry {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> String {
        if chunk.is_empty() && self.pending.is_empty() {
            return String::new();
        }
        if self.pending.is_empty() {
            return decode_or_hold(chunk, &mut self.pending);
        }
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(chunk);
        decode_or_hold(&buf, &mut self.pending)
    }

    /// Flush leftover bytes at stream end. Incomplete tails become `�`
    /// only here — never mid-stream.
    pub fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        let leftover = std::mem::take(&mut self.pending);
        String::from_utf8_lossy(&leftover).into_owned()
    }
}

fn decode_or_hold(bytes: &[u8], pending: &mut Vec<u8>) -> String {
    let mut out = String::new();
    let mut start = 0;
    while start < bytes.len() {
        match std::str::from_utf8(&bytes[start..]) {
            Ok(s) => {
                out.push_str(s);
                return out;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                if valid > 0 {
                    out.push_str(
                        std::str::from_utf8(&bytes[start..start + valid])
                            .expect("valid_up_to is valid UTF-8"),
                    );
                }
                if e.error_len().is_none() {
                    pending.extend_from_slice(&bytes[start + valid..]);
                    return out;
                }
                // Invalid sequence: skip it, keep going.
                start += valid + e.error_len().unwrap();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_chinese_reassembles() {
        let text = "你好世界";
        let bytes = text.as_bytes();
        assert!(bytes.len() > 4);
        // Split inside the second character (好 = E5 A5 BD).
        let mid = 4;
        assert!(std::str::from_utf8(&bytes[..mid]).is_err());

        let mut d = Utf8Carry::new();
        let a = d.push(&bytes[..mid]);
        let b = d.push(&bytes[mid..]);
        let tail = d.finish();
        assert_eq!(format!("{a}{b}{tail}"), text);
        assert!(!format!("{a}{b}{tail}").contains('\u{FFFD}'));
    }

    #[test]
    fn every_byte_boundary() {
        let text = "按你那份《对外口径》写一条。";
        let bytes = text.as_bytes();
        for mid in 1..bytes.len() {
            let mut d = Utf8Carry::new();
            let out = format!(
                "{}{}{}",
                d.push(&bytes[..mid]),
                d.push(&bytes[mid..]),
                d.finish()
            );
            assert_eq!(out, text, "split at {mid}");
        }
    }

    #[test]
    fn valid_chunk_passthrough() {
        let mut d = Utf8Carry::new();
        assert_eq!(d.push("hello".as_bytes()), "hello");
        assert!(d.finish().is_empty());
    }
}
