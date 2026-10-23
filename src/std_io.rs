//! Adapters for standard-library files, sockets, and locked standard streams.
use crate::{Presence, Sink};
use std::io::{self, IoSlice, Write};

/// Adapt a standard-library writer to the fast_io output protocol.
///
/// For stdout/stderr, pass a lock to hold it across the entire print operation.
/// Borrowing a writer (`Io::new(&mut writer)`) is supported.
#[derive(Debug)]
pub struct Io<W> {
    inner: W,
}
impl<W> Io<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
}
impl<W: Write> Sink for Io<W> {
    type Error = io::Error;
    #[inline]
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.write_all(bytes)
    }

    fn write_scatter(&mut self, fragments: &[&[u8]]) -> impl Presence<io::Result<()>> {
        // Bound stack storage and stay comfortably below OS iovec limits.
        // IoSlice's Windows representation has a 32-bit length field.
        let mut pieces = fragments.iter().flat_map(|p| p.chunks(u32::MAX as usize));
        loop {
            let mut slices = [IoSlice::new(&[]); 16];
            let mut len = 0;
            for slot in &mut slices {
                let Some(bytes) = pieces.next() else {
                    break;
                };
                *slot = IoSlice::new(bytes);
                len += 1;
            }
            if len == 0 {
                return Ok(());
            }
            let mut pending = &mut slices[..len];
            while !pending.is_empty() {
                match self.inner.write_vectored(pending) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                    Ok(n) => IoSlice::advance_slices(&mut pending, n),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        }
    }
    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
