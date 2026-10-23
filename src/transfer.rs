//! Stream transfer using the source's existing buffer.
use crate::{Presence, Sink, Source};
use core::fmt;

#[derive(Debug)]
pub enum CopyError<R, W> {
    Source(R),
    Sink(W),
    LengthOverflow,
}
impl<R: fmt::Display, W: fmt::Display> fmt::Display for CopyError<R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(f, "input error: {error}"),
            Self::Sink(error) => write!(f, "output error: {error}"),
            Self::LengthOverflow => f.write_str("copied byte count exceeds u64::MAX"),
        }
    }
}
impl<R: core::error::Error + 'static, W: core::error::Error + 'static> core::error::Error
    for CopyError<R, W>
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Sink(error) => Some(error),
            Self::LengthOverflow => None,
        }
    }
}

/// Copy until EOF, returning the byte count without flushing the output.
///
/// Writes borrow the source buffer directly; this function allocates no scratch
/// storage. Each input chunk is consumed only after a successful output write.
/// A failing output may already have written a prefix of its unconsumed chunk;
/// retrying the operation can therefore duplicate output.
pub fn copy<R: Source + ?Sized, W: Sink + ?Sized>(
    input: &mut R,
    output: &mut W,
) -> Result<u64, CopyError<R::Error, W::Error>> {
    let mut total = 0u64;
    loop {
        let bytes = input.fill_buf().map_err(CopyError::Source)?;
        if bytes.is_empty() {
            return Ok(total);
        }
        let len = bytes.len();
        let next = u64::try_from(len)
            .ok()
            .and_then(|len| total.checked_add(len))
            .ok_or(CopyError::LengthOverflow)?;
        output.write_all(bytes).map_err(CopyError::Sink)?;
        input.consume(len);
        total = next;
    }
}

/// A source view that exposes at most a specified number of bytes.
///
/// The underlying source may buffer more bytes, but this view never consumes
/// bytes beyond its limit. Recover the source with `into_inner` to keep reading.
#[derive(Debug)]
pub struct Take<S> {
    inner: S,
    remaining: u64,
}

pub fn take<S: Source>(source: S, limit: u64) -> Take<S> {
    Take {
        inner: source,
        remaining: limit,
    }
}
impl<S> Take<S> {
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
    pub fn get_ref(&self) -> &S {
        &self.inner
    }
    pub fn into_inner(self) -> S {
        self.inner
    }
}
impl<S: Source> Source for Take<S> {
    type Error = S::Error;
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        if self.remaining == 0 {
            return Ok(&[]);
        }
        let bytes = self.inner.fill_buf()?;
        let limit = usize::try_from(self.remaining).unwrap_or(usize::MAX);
        Ok(&bytes[..bytes.len().min(limit)])
    }
    fn consume(&mut self, count: usize) {
        let count64 = u64::try_from(count).expect("consume count exceeds u64::MAX");
        assert!(count64 <= self.remaining, "consumed beyond source limit");
        self.inner.consume(count);
        self.remaining -= count64;
    }
    fn read_into(&mut self, destination: &mut [u8]) -> impl Presence<Result<usize, Self::Error>> {
        let limit = usize::try_from(self.remaining).unwrap_or(usize::MAX);
        let len = destination.len().min(limit);
        let count = crate::read(&mut self.inner, &mut destination[..len])?;
        self.remaining -= count as u64;
        Ok(count)
    }
}
