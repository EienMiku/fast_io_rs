//! Owned pending bytes recovered from an input or output buffer.
use crate::{Encode, Presence, Sink, Source};

/// Fixed-capacity storage returned by a buffer's `into_parts` method.
///
/// It contains only the logical unread/unwritten range, exposed by `as_bytes`.
/// It can be read as a `Source` or written as an `Encode` value without allocating.
#[derive(Clone, Debug)]
pub struct BufferedBytes<const N: usize> {
    pub(crate) buffer: [u8; N],
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl<const N: usize> BufferedBytes<N> {
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer[self.start..self.end]
    }
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> AsRef<[u8]> for BufferedBytes<N> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<const N: usize> Source for BufferedBytes<N> {
    type Error = core::convert::Infallible;
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        Ok(self.as_bytes())
    }
    fn consume(&mut self, count: usize) {
        assert!(count <= self.len(), "consumed beyond recovered buffer");
        self.start += count;
    }
    fn buffered_bytes(&self) -> impl Presence<&[u8]> {
        self.as_bytes()
    }
}

impl<const N: usize> Encode for BufferedBytes<N> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self.as_bytes())
    }
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.as_bytes()
    }
}
