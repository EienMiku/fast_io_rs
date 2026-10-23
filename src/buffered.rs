//! Persistent output buffering between print calls.
use crate::{Presence, Sink, write_scatter};

/// A fixed-capacity buffer around an owned sink.
///
/// Large writes bypass an empty buffer. Call `finish` or `flush` to observe
/// errors; Drop only makes a best-effort flush. After an output error, pending
/// bytes are discarded because the sink may have consumed an unknown prefix.
#[derive(Debug)]
#[must_use = "call finish or flush to observe buffered output errors"]
pub struct Buffered<S: Sink, const N: usize = 8192> {
    inner: Option<S>,
    buffer: [u8; N],
    len: usize,
}
impl<S: Sink, const N: usize> Buffered<S, N> {
    pub fn new(inner: S) -> Self {
        Self {
            inner: Some(inner),
            buffer: [0; N],
            len: 0,
        }
    }
    pub fn get_ref(&self) -> &S {
        self.inner.as_ref().expect("buffer has been finished")
    }
    /// Access the inner sink. Flush first if you intend to write through it.
    pub fn get_mut(&mut self) -> &mut S {
        self.inner.as_mut().expect("buffer has been finished")
    }
    pub fn pending(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Send buffered bytes without calling the inner sink's flush.
    pub fn drain(&mut self) -> Result<(), S::Error> {
        let len = core::mem::take(&mut self.len);
        if len == 0 {
            return Ok(());
        }
        self.inner
            .as_mut()
            .expect("buffer has been finished")
            .write_all(&self.buffer[..len])
    }

    /// Flush and return the inner sink. On failure, the owned sink is dropped.
    pub fn finish(mut self) -> Result<S, S::Error> {
        let result = self.flush();
        let inner = self.inner.take().expect("buffer has been finished");
        result.map(|()| inner)
    }
}
impl<S: Sink, const N: usize> Sink for Buffered<S, N> {
    type Error = S::Error;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        if bytes.len() > N - self.len {
            self.drain()?;
        }
        if bytes.len() >= N {
            return self.get_mut().write_all(bytes);
        }
        self.buffer[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
    fn write_scatter(&mut self, fragments: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        let total = fragments
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()));
        if total.is_some_and(|n| n <= N - self.len) {
            for bytes in fragments {
                self.write_all(bytes)?;
            }
            return Ok(());
        }
        self.drain()?;
        if total.is_some_and(|n| n < N) {
            for bytes in fragments {
                self.write_all(bytes)?;
            }
            Ok(())
        } else {
            write_scatter(self.get_mut(), fragments)
        }
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.drain()?;
        self.get_mut().flush()
    }
}
impl<S: Sink, const N: usize> Drop for Buffered<S, N> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            let _ = self.flush();
        }
    }
}
