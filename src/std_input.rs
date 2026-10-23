//! Standard-library buffered input.
use crate::Source;
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
    pub fn from_buffered(reader: BufReader<R>) -> Self {
        Self { inner: reader }
    }
    pub fn into_inner(self) -> BufReader<R> {
        self.inner
    }
}
impl<R: Read> Source for Input<R> {
    type Error = io::Error;
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
}
