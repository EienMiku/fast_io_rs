//! JSON-compatible quoted string formatting.
use crate::{Encode, Presence, Sink, SliceSink};

/// A UTF-8 string enclosed in quotes, with JSON string escapes.
///
/// Non-ASCII text is preserved as UTF-8. Control characters, quote and backslash
/// are escaped. This formats a string value, not a complete JSON serializer.
#[derive(Clone, Copy, Debug)]
pub struct Quoted<'a>(pub &'a str);
pub fn quoted(text: &str) -> Quoted<'_> {
    Quoted(text)
}

fn short_escape(byte: u8) -> Option<u8> {
    match byte {
        b'"' => Some(b'"'),
        b'\\' => Some(b'\\'),
        b'\n' => Some(b'n'),
        b'\r' => Some(b'r'),
        b'\t' => Some(b't'),
        8 => Some(b'b'),
        12 => Some(b'f'),
        _ => None,
    }
}
impl Encode for Quoted<'_> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(b"\"")?;
        let bytes = self.0.as_bytes();
        let mut start = 0;
        for (index, &byte) in bytes.iter().enumerate() {
            let short = short_escape(byte);
            if short.is_none() && byte >= 32 {
                continue;
            }
            sink.write_all(&bytes[start..index])?;
            if let Some(escaped) = short {
                sink.write_all(&[b'\\', escaped])?;
            } else {
                let hex = b"0123456789abcdef";
                sink.write_all(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    hex[(byte >> 4) as usize],
                    hex[(byte & 15) as usize],
                ])?;
            }
            start = index + 1;
        }
        sink.write_all(&bytes[start..])?;
        sink.write_all(b"\"")
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        self.0
            .bytes()
            .try_fold(2usize, |len, byte| {
                len.checked_add(if short_escape(byte).is_some() {
                    2
                } else if byte < 32 {
                    6
                } else {
                    1
                })
            })
            .expect("quoted string output length overflow")
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        let mut sink = SliceSink::new(output);
        self.encode(&mut sink)
            .expect("quoted output buffer is too small");
        sink.len()
    }
}
