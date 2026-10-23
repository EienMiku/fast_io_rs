//! Explicit-endian binary I/O for primitive integers and floating-point values.
//!
//! These adapters preserve bit patterns and do not use text formatting. usize
//! and isize use the target's native width; prefer fixed-width types for files
//! and network protocols.
use crate::{Encode, Presence, Sink, Source};
use core::fmt;

mod sealed {
    pub trait Sealed {}
}
/// Primitive values for which every bit pattern is valid.
pub trait Binary: sealed::Sealed + Copy {
    const WIDTH: usize;
    #[doc(hidden)]
    fn from_ordered(bytes: &[u8], big_endian: bool) -> Self;
    #[doc(hidden)]
    fn write_ordered(self, output: &mut [u8], big_endian: bool);
}
macro_rules! primitives {
    ($($t:ty),*) => {$(
        impl sealed::Sealed for $t {}
        impl Binary for $t {
            const WIDTH: usize = core::mem::size_of::<Self>();
            fn from_ordered(bytes: &[u8], big_endian: bool) -> Self {
                let bytes = bytes.try_into().expect("binary input width");
                if big_endian { Self::from_be_bytes(bytes) } else { Self::from_le_bytes(bytes) }
            }
            fn write_ordered(self, output: &mut [u8], big_endian: bool) {
                let bytes = if big_endian { self.to_be_bytes() } else { self.to_le_bytes() };
                output[..Self::WIDTH].copy_from_slice(&bytes);
            }
        }
    )*};
}
primitives!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64
);

#[derive(Clone, Copy, Debug)]
pub struct Endian<T, const BIG_ENDIAN: bool>(pub T);
pub fn le<T: Binary>(value: T) -> Endian<T, false> {
    Endian(value)
}
pub fn be<T: Binary>(value: T) -> Endian<T, true> {
    Endian(value)
}
impl<T: Binary, const BIG: bool> Encode for Endian<T, BIG> {
    fn encode<S: Sink + ?Sized>(&self, output: &mut S) -> Result<(), S::Error> {
        let mut bytes = [0; 16];
        self.0.write_ordered(&mut bytes, BIG);
        output.write_all(&bytes[..T::WIDTH])
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        T::WIDTH
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        self.0.write_ordered(output, BIG);
        T::WIDTH
    }
}

#[derive(Debug)]
pub enum ReadError<E> {
    Source(E),
    UnexpectedEof { expected: usize, actual: usize },
}
impl<E: fmt::Display> fmt::Display for ReadError<E> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(out, "input error: {error}"),
            Self::UnexpectedEof { expected, actual } => {
                write!(out, "expected {expected} bytes, got {actual} before EOF")
            }
        }
    }
}
#[cfg(feature = "std")]
impl<E: std::error::Error + 'static> std::error::Error for ReadError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

/// Read one little-endian value. Clean EOF is None; a truncated value is an error.
/// Bytes already consumed before an input error cannot be rolled back.
pub fn read_le<T: Binary, S: Source + ?Sized>(
    input: &mut S,
) -> Result<Option<T>, ReadError<S::Error>> {
    read::<T, S, false>(input)
}
/// Read one big-endian value, with the same EOF/error contract as read_le.
pub fn read_be<T: Binary, S: Source + ?Sized>(
    input: &mut S,
) -> Result<Option<T>, ReadError<S::Error>> {
    read::<T, S, true>(input)
}
fn read<T: Binary, S: Source + ?Sized, const BIG: bool>(
    input: &mut S,
) -> Result<Option<T>, ReadError<S::Error>> {
    let mut scratch = [0; 16];
    let mut used = 0;
    loop {
        let bytes = input.fill_buf().map_err(ReadError::Source)?;
        if bytes.is_empty() {
            return if used == 0 {
                Ok(None)
            } else {
                Err(ReadError::UnexpectedEof {
                    expected: T::WIDTH,
                    actual: used,
                })
            };
        }
        if used == 0 && bytes.len() >= T::WIDTH {
            let value = T::from_ordered(&bytes[..T::WIDTH], BIG);
            input.consume(T::WIDTH);
            return Ok(Some(value));
        }
        let n = (T::WIDTH - used).min(bytes.len());
        scratch[used..used + n].copy_from_slice(&bytes[..n]);
        input.consume(n);
        used += n;
        if used == T::WIDTH {
            return Ok(Some(T::from_ordered(&scratch[..used], BIG)));
        }
    }
}
