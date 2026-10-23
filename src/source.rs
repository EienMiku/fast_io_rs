//! Buffered byte input, independent of std.
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
