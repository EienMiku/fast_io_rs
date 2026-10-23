//! Byte-oriented output devices.
use crate::{Presence, presence::missing};

/// A statically dispatched output device.
///
/// `write_all` consumes the whole slice on success. On error, a prefix may have
/// been written; callers must not retry the entire input blindly.
pub trait Sink {
    type Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;

    /// Optionally specialize a complete print operation for this device.
    ///
    /// Memory destinations can bypass scatter planning and write directly.
    fn write_arguments<A: crate::arguments::Arguments>(
        &mut self,
        _arguments: &A,
    ) -> impl Presence<Result<(), Self::Error>> {
        missing()
    }

    /// Optionally write all fragments in order, with the same error contract.
    ///
    /// The fragments remain borrowed only for this call. Devices can implement
    /// this with vectored I/O, or omit it to use the scalar fallback.
    fn write_scatter(&mut self, _fragments: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        missing()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<S: Sink + ?Sized> Sink for &mut S {
    type Error = S::Error;
    #[inline]
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        S::write_all(self, bytes)
    }
    #[inline]
    fn write_arguments<A: crate::arguments::Arguments>(
        &mut self,
        arguments: &A,
    ) -> impl Presence<Result<(), Self::Error>> {
        S::write_arguments(self, arguments)
    }
    #[inline]
    fn write_scatter(&mut self, fragments: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        S::write_scatter(self, fragments)
    }
    #[inline]
    fn flush(&mut self) -> Result<(), Self::Error> {
        S::flush(self)
    }
}

/// Write a sequence of fragments, selecting a device's scatter hook if present.
#[inline]
pub fn write_scatter<S: Sink + ?Sized>(sink: &mut S, fragments: &[&[u8]]) -> Result<(), S::Error> {
    if const { crate::provided2(S::write_scatter) } {
        sink.write_scatter(fragments).into_value()
    } else {
        for bytes in fragments {
            if !bytes.is_empty() {
                sink.write_all(bytes)?;
            }
        }
        Ok(())
    }
}
