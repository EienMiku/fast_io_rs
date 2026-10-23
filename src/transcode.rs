//! Allocation-free UTF-16 / UTF-8 formatting adapters.
use crate::{Encode, Presence, Sink};
use core::fmt;

/// An unpaired surrogate in a UTF-16 input slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Utf16Error {
    pub index: usize,
    pub unit: u16,
}
impl fmt::Display for Utf16Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "unpaired UTF-16 surrogate {:#06x} at unit {}",
            self.unit, self.index
        )
    }
}
#[cfg(feature = "std")]
impl std::error::Error for Utf16Error {}

/// Validated UTF-16 input, formatted as UTF-8.
#[derive(Clone, Copy, Debug)]
pub struct Utf16<'a>(&'a [u16]);

/// UTF-16 input formatted as UTF-8, replacing unpaired surrogates with U+FFFD.
#[derive(Clone, Copy, Debug)]
pub struct Utf16Lossy<'a>(&'a [u16]);

pub fn utf16(units: &[u16]) -> Result<Utf16<'_>, Utf16Error> {
    let mut index = 0;
    for character in char::decode_utf16(units.iter().copied()) {
        let character = character.map_err(|error| Utf16Error {
            index,
            unit: error.unpaired_surrogate(),
        })?;
        index += character.len_utf16();
    }
    Ok(Utf16(units))
}
pub fn utf16_lossy(units: &[u16]) -> Utf16Lossy<'_> {
    Utf16Lossy(units)
}

fn size_bound(units: &[u16]) -> usize {
    units
        .len()
        .checked_mul(3)
        .expect("UTF-16 conversion length overflow")
}
fn characters<const LOSSY: bool>(units: &[u16]) -> impl Iterator<Item = char> + '_ {
    char::decode_utf16(units.iter().copied()).map(|character| match character {
        Ok(c) => c,
        Err(_) if LOSSY => char::REPLACEMENT_CHARACTER,
        Err(_) => unreachable!("Utf16 input was validated and is immutably borrowed"),
    })
}
fn encode_utf8<const LOSSY: bool>(units: &[u16], output: &mut [u8]) -> usize {
    let mut written = 0;
    for character in characters::<LOSSY>(units) {
        written += character.encode_utf8(&mut output[written..]).len();
    }
    written
}
fn stream_utf8<S: Sink + ?Sized, const LOSSY: bool>(
    units: &[u16],
    sink: &mut S,
) -> Result<(), S::Error> {
    let mut buffer = [0; 256];
    let mut used = 0;
    for character in characters::<LOSSY>(units) {
        if buffer.len() - used < 4 {
            sink.write_all(&buffer[..used])?;
            used = 0;
        }
        used += character.encode_utf8(&mut buffer[used..]).len();
    }
    if used != 0 {
        sink.write_all(&buffer[..used])?;
    }
    Ok(())
}
macro_rules! from_utf16 {
    ($ty:ident, $lossy:literal) => {
        impl Encode for $ty<'_> {
            fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
                stream_utf8::<S, $lossy>(self.0, sink)
            }
            fn reserve_size(&self) -> impl Presence<usize> {
                size_bound(self.0)
            }
            fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
                encode_utf8::<$lossy>(self.0, output)
            }
        }
    };
}
from_utf16!(Utf16, false);
from_utf16!(Utf16Lossy, true);

/// A UTF-8 string encoded as UTF-16 bytes, without a byte-order mark.
#[derive(Clone, Copy, Debug)]
pub struct Utf16Bytes<'a, const BIG_ENDIAN: bool>(&'a str);

pub fn utf16_le(text: &str) -> Utf16Bytes<'_, false> {
    Utf16Bytes(text)
}
pub fn utf16_be(text: &str) -> Utf16Bytes<'_, true> {
    Utf16Bytes(text)
}

impl<const BIG: bool> Encode for Utf16Bytes<'_, BIG> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        let mut buffer = [0; 256];
        let mut used = 0;
        for unit in self.0.encode_utf16() {
            if used == buffer.len() {
                sink.write_all(&buffer)?;
                used = 0;
            }
            let bytes = if BIG {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            };
            buffer[used..used + 2].copy_from_slice(&bytes);
            used += 2;
        }
        if used != 0 {
            sink.write_all(&buffer[..used])?;
        }
        Ok(())
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        self.0
            .len()
            .checked_mul(2)
            .expect("UTF-16 conversion length overflow")
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        let mut used = 0;
        for unit in self.0.encode_utf16() {
            let bytes = if BIG {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            };
            output[used..used + 2].copy_from_slice(&bytes);
            used += 2;
        }
        used
    }
}
