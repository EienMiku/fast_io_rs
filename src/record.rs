//! Bounded delimiter-separated records and UTF-8 lines.
use crate::Source;
use core::fmt;

#[derive(Debug)]
pub enum RecordError<E> {
    Source(E),
    TooLong { limit: usize },
    InvalidUtf8,
}
impl<E: fmt::Display> fmt::Display for RecordError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(f, "input error: {error}"),
            Self::TooLong { limit } => write!(f, "record exceeds {limit} bytes"),
            Self::InvalidUtf8 => f.write_str("line is not valid UTF-8"),
        }
    }
}
impl<E: core::error::Error + 'static> core::error::Error for RecordError<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

/// Read one record into caller-owned storage, excluding its delimiter.
///
/// Returns the payload length, or None at clean EOF. A final unterminated record
/// is returned normally. The delimiter is consumed, including after an empty or
/// oversized record. On TooLong, the rest of that record is discarded so the
/// next call can recover. On a source error, part of the record may be consumed.
/// Buffer contents after an error are unspecified.
pub fn read_record<S: Source + ?Sized>(
    input: &mut S,
    delimiter: u8,
    buffer: &mut [u8],
) -> Result<Option<usize>, RecordError<S::Error>> {
    read_record_inner(input, delimiter, buffer).map(|record| record.map(|(len, _)| len))
}

/// Read a UTF-8 line into caller-owned storage, removing LF or CRLF.
///
/// The returned view borrows the output buffer. A bare CR at EOF is preserved.
/// Capacity includes a CR before LF, but excludes LF itself. Oversized and
/// invalid UTF-8 lines are consumed so a subsequent call starts at the next line.
pub fn read_line<'b, S: Source + ?Sized>(
    input: &mut S,
    buffer: &'b mut [u8],
) -> Result<Option<&'b str>, RecordError<S::Error>> {
    let Some((mut len, terminated)) = read_record_inner(input, b'\n', buffer)? else {
        return Ok(None);
    };
    if terminated && len != 0 && buffer[len - 1] == b'\r' {
        len -= 1;
    }
    core::str::from_utf8(&buffer[..len])
        .map(Some)
        .map_err(|_| RecordError::InvalidUtf8)
}

fn read_record_inner<S: Source + ?Sized>(
    input: &mut S,
    delimiter: u8,
    buffer: &mut [u8],
) -> Result<Option<(usize, bool)>, RecordError<S::Error>> {
    let mut used = 0;
    let mut started = false;
    let mut too_long = false;
    loop {
        let bytes = input.fill_buf().map_err(RecordError::Source)?;
        if bytes.is_empty() {
            return if too_long {
                Err(RecordError::TooLong {
                    limit: buffer.len(),
                })
            } else if started {
                Ok(Some((used, false)))
            } else {
                Ok(None)
            };
        }
        let end = bytes.iter().position(|&byte| byte == delimiter);
        let len = end.unwrap_or(bytes.len());
        started = true;
        if !too_long {
            if len > buffer.len() - used {
                too_long = true;
            } else {
                buffer[used..used + len].copy_from_slice(&bytes[..len]);
                used += len;
            }
        }
        input.consume(len + usize::from(end.is_some()));
        if end.is_some() {
            return if too_long {
                Err(RecordError::TooLong {
                    limit: buffer.len(),
                })
            } else {
                Ok(Some((used, true)))
            };
        }
    }
}
