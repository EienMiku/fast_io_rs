//! Formatting protocol and single-value dispatch.
use crate::{Presence, Sink, presence::missing};

/// A value that can be encoded as bytes.
///
/// Implement `encode` for the general case. Override optional methods to enable
/// faster paths; no separate capability flag is required. All paths must emit
/// identical bytes for the same value.
pub trait Encode {
    /// Stream the representation to a device.
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error>;

    /// Borrow the complete encoded representation without copying.
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        missing()
    }

    /// An upper bound for this value's reserved representation, in bytes.
    ///
    /// A reserved path is used only when both this and `encode_reserved` exist.
    /// The bound may depend on the value; querying it must not consume the value.
    fn reserve_size(&self) -> impl Presence<usize> {
        missing()
    }

    /// Write into a slice of at least `reserve_size()` bytes, returning bytes used.
    ///
    /// The output starts at offset zero. The return value must not exceed the
    /// supplied slice length. Internal consumers retain bounds checks.
    fn encode_reserved(&self, _buffer: &mut [u8]) -> impl Presence<usize> {
        missing()
    }

    /// Optionally append this value's fields directly to a shared batch.
    ///
    /// Borrowed fields can remain borrowed across the whole outer print call.
    /// Use `batch.write_all` for temporary bytes that cannot outlive this call.
    fn encode_batch<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        _batch: &mut crate::Batch<'_, 'v, S, B, P>,
    ) -> impl Presence<Result<(), S::Error>> {
        missing()
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    #[inline]
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        T::encode(self, sink)
    }
    #[inline]
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        T::borrowed_bytes(self)
    }
    #[inline]
    fn reserve_size(&self) -> impl Presence<usize> {
        T::reserve_size(self)
    }
    #[inline]
    fn encode_reserved(&self, buffer: &mut [u8]) -> impl Presence<usize> {
        T::encode_reserved(self, buffer)
    }

    #[inline]
    fn encode_batch<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut crate::Batch<'_, 'v, S, B, P>,
    ) -> impl Presence<Result<(), S::Error>> {
        T::encode_batch(self, batch)
    }
}

/// Format one value, using borrowed or small reserved output when available.
///
/// This function does not flush the device. Large reserved values use their
/// streaming fallback instead of allocating.
#[inline]
pub fn write<S: Sink + ?Sized, T: Encode + ?Sized>(
    sink: &mut S,
    value: &T,
) -> Result<(), S::Error> {
    if const { crate::provided(T::borrowed_bytes) } {
        sink.write_all(value.borrowed_bytes().into_value())
    } else if const { crate::provided(T::reserve_size) && crate::provided2(T::encode_reserved) } {
        let size = value.reserve_size().into_value();
        let mut scratch = [0u8; 128];
        if size <= scratch.len() {
            let buffer = &mut scratch[..size];
            let used = value.encode_reserved(buffer).into_value();
            sink.write_all(&buffer[..used])
        } else {
            value.encode(sink)
        }
    } else {
        value.encode(sink)
    }
}
