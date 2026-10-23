//! Standard-library buffered input.
use crate::{Presence, Source};
use std::io::{self, BufRead, BufReader, Read};

/// Buffered input that retries interrupted reads.
///
/// The underlying BufReader is returned by into_inner so prefetched, unread
/// bytes are preserved.
#[derive(Debug)]
pub struct Input<R> {
    inner: BufReader<R>,
}
impl<R: Read> Input<R> {
    pub fn new(reader: R) -> Self {
        Self {
            inner: BufReader::new(reader),
        }
    }
    pub fn with_capacity(capacity: usize, reader: R) -> Self {
        assert!(capacity > 0, "input buffer capacity must be positive");
        Self {
            inner: BufReader::with_capacity(capacity, reader),
        }
    }
    pub fn get_ref(&self) -> &R {
        self.inner.get_ref()
    }
    /// Reuse an existing BufReader, including its unread prefetched bytes.
    /// No extra buffer is allocated around it.
    ///
    /// Panics if its capacity is zero: a zero-capacity BufReader cannot expose
    /// input through `fill_buf` and would make scanners report premature EOF.
    pub fn from_buffered(reader: BufReader<R>) -> Self {
        assert!(
            reader.capacity() > 0,
            "input buffer capacity must be positive"
        );
        Self { inner: reader }
    }
    pub fn into_inner(self) -> BufReader<R> {
        self.inner
    }
}
impl<R: Read> Source for Input<R> {
    type Error = io::Error;
    #[inline]
    fn buffered_bytes(&self) -> impl Presence<&[u8]> {
        self.inner.buffer()
    }
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        loop {
            match self.inner.fill_buf() {
                Ok(_) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(self.inner.buffer())
    }
    fn consume(&mut self, count: usize) {
        assert!(
            count <= self.inner.buffer().len(),
            "consumed beyond input buffer"
        );
        self.inner.consume(count);
    }
    fn read_into(&mut self, destination: &mut [u8]) -> impl Presence<io::Result<usize>> {
        loop {
            match self.inner.read(destination) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

impl<R: Read> Read for Input<R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        crate::read(self, destination)
    }
}

impl<R: Read> BufRead for Input<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Source::fill_buf(self)
    }
    fn consume(&mut self, count: usize) {
        Source::consume(self, count);
    }
}

impl<R: crate::Reader<Error = io::Error>, const N: usize> Read for crate::BufferedInput<R, N> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        crate::read(self, destination)
    }
}

impl<R: crate::Reader<Error = io::Error>, const N: usize> BufRead for crate::BufferedInput<R, N> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Source::fill_buf(self)
    }
    fn consume(&mut self, count: usize) {
        Source::consume(self, count);
    }
}
