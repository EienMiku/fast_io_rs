//! In-memory output devices.
use crate::{Presence, Sink};
use core::fmt;

/// A fixed output buffer was too small. The failed write does not modify it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityError {
    pub required: usize,
    pub remaining: usize,
}
impl fmt::Display for CapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "need {} bytes, but only {} remain",
            self.required, self.remaining
        )
    }
}
#[cfg(feature = "std")]
impl std::error::Error for CapacityError {}

/// Output into caller-owned storage without allocation.
#[derive(Debug)]
pub struct SliceSink<'a> {
    buffer: &'a mut [u8],
    len: usize,
}
impl<'a> SliceSink<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, len: 0 }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn remaining(&self) -> usize {
        self.buffer.len() - self.len
    }
    pub fn clear(&mut self) {
        self.len = 0;
    }
    pub fn into_written(self) -> &'a mut [u8] {
        &mut self.buffer[..self.len]
    }
}
impl Sink for SliceSink<'_> {
    type Error = CapacityError;
    #[inline]
    fn write_arguments<A: crate::arguments::Arguments>(
        &mut self,
        arguments: &A,
    ) -> impl Presence<Result<(), Self::Error>> {
        if let Some(size) = arguments
            .reserve_size()
            .filter(|&size| size <= self.remaining())
        {
            let used = arguments.encode_reserved(&mut self.buffer[self.len..self.len + size]);
            assert!(
                used <= size,
                "argument sequence exceeded its reserved output bound"
            );
            self.len += used;
            return Ok(());
        }
        arguments.stream(self)
    }
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        if bytes.len() > self.remaining() {
            return Err(CapacityError {
                required: bytes.len(),
                remaining: self.remaining(),
            });
        }
        self.buffer[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
    fn write_scatter(&mut self, fragments: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        let required = fragments
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()));
        let required = required.unwrap_or(usize::MAX);
        if required > self.remaining() {
            return Err(CapacityError {
                required,
                remaining: self.remaining(),
            });
        }
        for bytes in fragments {
            self.write_all(bytes)?;
        }
        Ok(())
    }
}

#[cfg(feature = "alloc")]
impl Sink for alloc::vec::Vec<u8> {
    type Error = core::convert::Infallible;
    #[inline]
    fn write_arguments<A: crate::arguments::Arguments>(
        &mut self,
        arguments: &A,
    ) -> impl Presence<Result<(), Self::Error>> {
        if let Some(size) = arguments.reserve_size() {
            let start = self.len();
            let end = start
                .checked_add(size)
                .expect("combined output length overflow");
            let mut tail = VecTail {
                bytes: self,
                keep: start,
            };
            tail.bytes.resize(end, 0);
            let used = arguments.encode_reserved(&mut tail.bytes[start..]);
            assert!(
                used <= size,
                "argument sequence exceeded its reserved output bound"
            );
            tail.keep = start + used;
            return Ok(());
        }
        arguments.stream(self)
    }
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.extend_from_slice(bytes);
        Ok(())
    }
    fn write_scatter(&mut self, fragments: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        let total = fragments
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()))
            .expect("combined output length overflow");
        self.reserve(total);
        for bytes in fragments {
            self.extend_from_slice(bytes);
        }
        Ok(())
    }
}

// Restore the original length if a custom formatter panics mid-reservation.
#[cfg(feature = "alloc")]
struct VecTail<'a> {
    bytes: &'a mut alloc::vec::Vec<u8>,
    keep: usize,
}
#[cfg(feature = "alloc")]
impl Drop for VecTail<'_> {
    fn drop(&mut self) {
        self.bytes.truncate(self.keep);
    }
}
