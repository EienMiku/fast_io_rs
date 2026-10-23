//! Fixed-capacity input buffering without allocation or std.
use crate::{Presence, Source};

/// A device that can read bytes into caller-owned storage.
///
/// Return the number of bytes read, at most the destination length. Zero for a
/// nonempty destination indicates EOF; short reads are allowed. On error, no
/// input bytes may be consumed, but destination contents are unspecified.
/// Device-specific retryable errors should be handled by the implementation.
pub trait Reader {
    type Error;
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error>;
}

impl<R: Reader + ?Sized> Reader for &mut R {
    type Error = R::Error;
    #[inline]
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
        R::read(self, destination)
    }
}

impl Reader for &[u8] {
    type Error = core::convert::Infallible;
    #[inline]
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
        let count = destination.len().min(self.len());
        destination[..count].copy_from_slice(&self[..count]);
        *self = &self[count..];
        Ok(count)
    }
}

/// Turn a raw reader into a buffered source using inline, fixed-capacity storage.
///
/// This works without std or allocation. Large raw reads bypass an empty buffer;
/// unread prefetched bytes are returned first. Unlike `Input` (with
/// std), the buffer lives inside this value and has a compile-time capacity.
///
/// ```
/// use fast_io::{BufferedInput, Scanner};
/// let input = BufferedInput::<_, 4>::new(b"12345 -7".as_slice());
/// let mut scanner = Scanner::new(input);
/// assert_eq!(scanner.require::<u32>().unwrap(), 12345);
/// assert_eq!(scanner.require::<i8>().unwrap(), -7);
/// ```
#[derive(Debug)]
pub struct BufferedInput<R, const N: usize = 8192> {
    inner: R,
    buffer: [u8; N],
    position: usize,
    filled: usize,
}

impl<R: Reader, const N: usize> BufferedInput<R, N> {
    /// Panics if the capacity is zero.
    pub fn new(inner: R) -> Self {
        assert!(N > 0, "input buffer capacity must be positive");
        Self {
            inner,
            buffer: [0; N],
            position: 0,
            filled: 0,
        }
    }

    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Access the device. Reading through it bypasses unread prefetched bytes.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Inspect unread prefetched bytes without performing an input operation.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer[self.position..self.filled]
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    /// Return the device, discarding any unread prefetched bytes.
    ///
    /// Inspect `buffer()` first if those bytes must be retained. To keep reading
    /// in order, retain this buffered source instead of unwrapping it.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Reader, const N: usize> Source for BufferedInput<R, N> {
    type Error = R::Error;

    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        if self.position == self.filled {
            self.position = 0;
            self.filled = 0;
            let count = self.inner.read(&mut self.buffer)?;
            assert!(count <= N, "reader exceeded its input buffer");
            self.filled = count;
        }
        Ok(self.buffer())
    }

    fn consume(&mut self, count: usize) {
        assert!(
            count <= self.filled - self.position,
            "consumed beyond input buffer"
        );
        self.position += count;
    }

    fn read_into(&mut self, destination: &mut [u8]) -> impl Presence<Result<usize, Self::Error>> {
        if destination.is_empty() {
            return Ok(0);
        }
        if self.position == self.filled && destination.len() >= N {
            let count = self.inner.read(destination)?;
            assert!(
                count <= destination.len(),
                "reader exceeded its input buffer"
            );
            return Ok(count);
        }
        let bytes = self.fill_buf()?;
        let count = destination.len().min(bytes.len());
        destination[..count].copy_from_slice(&bytes[..count]);
        self.consume(count);
        Ok(count)
    }
}
