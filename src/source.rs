//! Buffered byte input, independent of std.
use crate::{Presence, ReadError, presence::missing};

/// A buffered byte source with optional direct reads into caller-owned storage.
pub trait Source {
    type Error;
    /// Expose the next bytes. An empty slice indicates end of input.
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error>;
    /// Consume at most the length of the last exposed slice.
    fn consume(&mut self, count: usize);

    /// Optionally read directly into the destination, consuming the bytes read.
    ///
    /// This must read the same ordered stream as `fill_buf`/`consume`, including
    /// any unread prefetched bytes. Return at most `destination.len()` bytes;
    /// zero for a nonempty destination means EOF. Short reads are allowed.
    /// On error, no input bytes may be consumed, but the destination's contents
    /// are unspecified. Callers must not call `consume` after this hook.
    fn read_into(&mut self, _destination: &mut [u8]) -> impl Presence<Result<usize, Self::Error>> {
        missing()
    }
}
impl<S: Source + ?Sized> Source for &mut S {
    type Error = S::Error;
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        S::fill_buf(self)
    }
    fn consume(&mut self, count: usize) {
        S::consume(self, count);
    }
    #[inline]
    fn read_into(&mut self, destination: &mut [u8]) -> impl Presence<Result<usize, Self::Error>> {
        S::read_into(self, destination)
    }
}
impl Source for &[u8] {
    type Error = core::convert::Infallible;
    #[inline]
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        Ok(self)
    }
    #[inline]
    fn consume(&mut self, count: usize) {
        *self = &self[count..];
    }
}

/// Copy available input into a caller-owned buffer, returning bytes read.
///
/// Uses the optional direct-read hook, or copies from one source buffer. A short
/// read does not imply EOF. Zero means EOF or an empty destination. An empty
/// destination does not touch the source. A faulty hook's byte count is checked.
#[inline]
pub fn read<S: Source + ?Sized>(source: &mut S, destination: &mut [u8]) -> Result<usize, S::Error> {
    if destination.is_empty() {
        return Ok(0);
    }
    if const { crate::provided2(S::read_into) } {
        let count = source.read_into(destination).into_value()?;
        assert!(
            count <= destination.len(),
            "source exceeded its read buffer"
        );
        return Ok(count);
    }
    let bytes = source.fill_buf()?;
    let count = destination.len().min(bytes.len());
    destination[..count].copy_from_slice(&bytes[..count]);
    source.consume(count);
    Ok(count)
}

/// Fill the entire destination, or report a source error or a truncated input.
///
/// On failure, an already copied prefix remains in the destination and those
/// input bytes remain consumed. After a source error, the rest of the destination
/// may also have been modified by the source's direct-read hook.
/// `UnexpectedEof` reports the destination size and the prefix length.
/// An empty destination succeeds without touching the source.
pub fn read_exact<S: Source + ?Sized>(
    source: &mut S,
    destination: &mut [u8],
) -> Result<(), ReadError<S::Error>> {
    let expected = destination.len();
    let mut actual = 0;
    while actual < expected {
        let count = read(source, &mut destination[actual..]).map_err(ReadError::Source)?;
        if count == 0 {
            return Err(ReadError::UnexpectedEof { expected, actual });
        }
        actual += count;
    }
    Ok(())
}
