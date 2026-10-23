//! Buffered byte input, independent of std.
use crate::ReadError;

pub trait Source {
    type Error;
    /// Expose the next bytes. An empty slice indicates end of input.
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error>;
    /// Consume at most the length of the last exposed slice.
    fn consume(&mut self, count: usize);
}
impl<S: Source + ?Sized> Source for &mut S {
    type Error = S::Error;
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        S::fill_buf(self)
    }
    fn consume(&mut self, count: usize) {
        S::consume(self, count);
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
/// Reads from one source buffer, so a short read does not imply EOF. Zero means
/// EOF or an empty destination. An empty destination does not touch the source.
pub fn read<S: Source + ?Sized>(source: &mut S, destination: &mut [u8]) -> Result<usize, S::Error> {
    if destination.is_empty() {
        return Ok(0);
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
/// input bytes remain consumed. The rest of the destination is unchanged.
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
