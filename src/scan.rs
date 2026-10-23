//! Whitespace-delimited parsing with bounded cross-buffer storage.
use crate::Source;
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    Empty,
    InvalidDigit { offset: usize },
    Overflow,
    InvalidUtf8,
    InvalidValue,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty token"),
            Self::InvalidDigit { offset } => write!(f, "invalid digit at token byte {offset}"),
            Self::Overflow => f.write_str("numeric value is out of range"),
            Self::InvalidUtf8 => f.write_str("token is not valid UTF-8"),
            Self::InvalidValue => f.write_str("invalid token value"),
        }
    }
}
impl core::error::Error for ParseError {}

#[derive(Debug)]
pub enum ScanError<E> {
    Source(E),
    Parse(ParseError),
    EndOfInput,
    TokenTooLong { limit: usize },
}
impl<E: fmt::Display> fmt::Display for ScanError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(e) => write!(f, "input error: {e}"),
            Self::Parse(e) => e.fmt(f),
            Self::EndOfInput => f.write_str("unexpected end of input"),
            Self::TokenTooLong { limit } => write!(f, "input token exceeds {limit} bytes"),
        }
    }
}
impl<E: core::error::Error + 'static> core::error::Error for ScanError<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Source(e) => Some(e),
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

/// Parse one complete token. Implementations cannot retain a borrow of it.
pub trait Scan: Sized {
    fn scan_token(token: &[u8]) -> Result<Self, ParseError>;
}

/// ASCII-whitespace token scanner with a configurable maximum token length.
///
/// Contiguous tokens are parsed directly from the source buffer. Only tokens
/// crossing source buffers are copied into bounded stack storage. Invalid and
/// oversized tokens are consumed completely, so a subsequent scan can recover.
/// A source error can occur after consuming part of a token; the caller should
/// treat that token as lost.
#[derive(Debug)]
pub struct Scanner<S, const TOKEN: usize = 256> {
    source: S,
}
impl<S: Source> Scanner<S, 256> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}
impl<S: Source, const N: usize> Scanner<S, N> {
    pub fn with_capacity(source: S) -> Self {
        Self { source }
    }
    pub fn get_ref(&self) -> &S {
        &self.source
    }
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.source
    }
    pub fn into_inner(self) -> S {
        self.source
    }

    #[inline]
    pub fn scan<T: Scan>(&mut self) -> Result<Option<T>, ScanError<S::Error>> {
        let mut scratch = [0; N];
        let mut used = 0;
        let mut started = false;
        let mut too_long = false;
        loop {
            let bytes = self.source.fill_buf().map_err(ScanError::Source)?;
            if bytes.is_empty() {
                if !started {
                    return Ok(None);
                }
                return if too_long {
                    Err(ScanError::TokenTooLong { limit: N })
                } else {
                    T::scan_token(&scratch[..used])
                        .map(Some)
                        .map_err(ScanError::Parse)
                };
            }
            let skip = if !started {
                bytes.iter().take_while(|c| c.is_ascii_whitespace()).count()
            } else {
                0
            };
            if skip == bytes.len() {
                self.source.consume(skip);
                continue;
            }
            let token = &bytes[skip..];
            let end = token
                .iter()
                .position(u8::is_ascii_whitespace)
                .unwrap_or(token.len());
            let complete = end < token.len();
            if !started && complete {
                let result = if end > N {
                    Err(ScanError::TokenTooLong { limit: N })
                } else {
                    T::scan_token(&token[..end])
                        .map(Some)
                        .map_err(ScanError::Parse)
                };
                self.source.consume(skip + end);
                return result;
            }
            started = true;
            if !too_long {
                if end > N - used {
                    too_long = true;
                } else {
                    scratch[used..used + end].copy_from_slice(&token[..end]);
                    used += end;
                }
            }
            self.source.consume(skip + end);
            if complete {
                return if too_long {
                    Err(ScanError::TokenTooLong { limit: N })
                } else {
                    T::scan_token(&scratch[..used])
                        .map(Some)
                        .map_err(ScanError::Parse)
                };
            }
        }
    }

    pub fn require<T: Scan>(&mut self) -> Result<T, ScanError<S::Error>> {
        self.scan()?.ok_or(ScanError::EndOfInput)
    }
}

macro_rules! unsigned {
    ($($ty:ty),*) => {$(
        impl Scan for $ty {
            #[inline]
            fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
                let start = usize::from(token.first() == Some(&b'+'));
                if token.len() == start { return Err(ParseError::Empty); }
                let mut value: Self = 0;
                // Every decimal value this short fits, so only validate digits.
                // floor(BITS * log10(2)) is a safe bound for all integer widths.
                if token.len() - start <= (<$ty>::BITS as usize * 30103 / 100000) {
                    for (index, &byte) in token[start..].iter().enumerate() {
                        let digit = byte.wrapping_sub(b'0');
                        if digit > 9 { return Err(ParseError::InvalidDigit { offset: start + index }); }
                        value = value.wrapping_mul(10).wrapping_add(digit as Self);
                    }
                    return Ok(value);
                }
                for (index, &byte) in token[start..].iter().enumerate() {
                    if !byte.is_ascii_digit() { return Err(ParseError::InvalidDigit { offset: start + index }); }
                    value = value.checked_mul(10)
                        .and_then(|n| n.checked_add((byte - b'0') as Self))
                        .ok_or(ParseError::Overflow)?;
                }
                Ok(value)
            }
        }
    )*};
}
macro_rules! signed {
    ($($ty:ty),*) => {$(
        impl Scan for $ty {
            #[inline]
            fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
                let negative = token.first() == Some(&b'-');
                let start = usize::from(negative || token.first() == Some(&b'+'));
                if token.len() == start { return Err(ParseError::Empty); }
                let mut value: Self = 0;
                if token.len() - start <= ((<$ty>::BITS as usize - 1) * 30103 / 100000) {
                    for (index, &byte) in token[start..].iter().enumerate() {
                        let digit = byte.wrapping_sub(b'0');
                        if digit > 9 { return Err(ParseError::InvalidDigit { offset: start + index }); }
                        value = value.wrapping_mul(10).wrapping_add(digit as Self);
                    }
                    return Ok(if negative { value.wrapping_neg() } else { value });
                }
                for (index, &byte) in token[start..].iter().enumerate() {
                    if !byte.is_ascii_digit() { return Err(ParseError::InvalidDigit { offset: start + index }); }
                    let multiplied = value.checked_mul(10).ok_or(ParseError::Overflow)?;
                    value = if negative {
                        multiplied.checked_sub((byte - b'0') as Self)
                    } else {
                        multiplied.checked_add((byte - b'0') as Self)
                    }.ok_or(ParseError::Overflow)?;
                }
                Ok(value)
            }
        }
    )*};
}
unsigned!(u8, u16, u32, u64, u128, usize);
signed!(i8, i16, i32, i64, i128, isize);

impl Scan for bool {
    fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
        match token {
            b"true" => Ok(true),
            b"false" => Ok(false),
            _ => Err(ParseError::InvalidValue),
        }
    }
}
impl Scan for char {
    fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
        let text = core::str::from_utf8(token).map_err(|_| ParseError::InvalidUtf8)?;
        let mut chars = text.chars();
        let first = chars.next().ok_or(ParseError::Empty)?;
        if chars.next().is_some() {
            Err(ParseError::InvalidValue)
        } else {
            Ok(first)
        }
    }
}
macro_rules! float {
    ($($ty:ty),*) => {$(
        impl Scan for $ty {
            fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
                let text = core::str::from_utf8(token).map_err(|_| ParseError::InvalidUtf8)?;
                text.parse().map_err(|_| ParseError::InvalidValue)
            }
        }
    )*};
}
float!(f32, f64);

#[cfg(feature = "alloc")]
impl Scan for alloc::string::String {
    fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
        core::str::from_utf8(token)
            .map(alloc::string::String::from)
            .map_err(|_| ParseError::InvalidUtf8)
    }
}
#[cfg(feature = "alloc")]
impl Scan for alloc::vec::Vec<u8> {
    fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
        Ok(token.to_vec())
    }
}

/// Read a typed tuple, requiring every token to exist.
#[macro_export]
macro_rules! scan {
    ($scanner:expr; $($ty:ty),+ $(,)?) => {{
        let scanner = $scanner;
        (|| Ok::<_, $crate::ScanError<_>>(($(scanner.require::<$ty>()?,)+)))()
    }};
}

impl<'a, const N: usize> Scanner<&'a [u8], N> {
    /// Borrow a token directly from the original slice, without allocation.
    ///
    /// The returned view may be retained while subsequent tokens are read.
    pub fn scan_bytes(&mut self) -> Result<Option<&'a [u8]>, ScanError<core::convert::Infallible>> {
        let input = self.source;
        let start = input.iter().take_while(|b| b.is_ascii_whitespace()).count();
        if start == input.len() {
            self.source = &input[start..];
            return Ok(None);
        }
        let end = start
            + input[start..]
                .iter()
                .position(u8::is_ascii_whitespace)
                .unwrap_or(input.len() - start);
        self.source = &input[end..];
        if end - start > N {
            return Err(ScanError::TokenTooLong { limit: N });
        }
        Ok(Some(&input[start..end]))
    }

    /// Borrow a validated UTF-8 token from the original slice.
    pub fn scan_str(&mut self) -> Result<Option<&'a str>, ScanError<core::convert::Infallible>> {
        self.scan_bytes()?
            .map(|bytes| {
                core::str::from_utf8(bytes).map_err(|_| ScanError::Parse(ParseError::InvalidUtf8))
            })
            .transpose()
    }
}
