//! Heterogeneous arguments for the print macros.
use crate::{Batch, Encode, Presence, Sink, provided, provided2};

/// An empty argument sequence, used by the exported macros.
#[doc(hidden)]
#[derive(Debug)]
pub struct End;

/// A borrowed argument followed by the rest of the sequence.
#[doc(hidden)]
#[derive(Debug)]
pub struct Cons<'a, H: ?Sized, T> {
    pub head: &'a H,
    pub tail: T,
}

/// A heterogeneous sequence whose elements can be statically dispatched.
#[doc(hidden)]
pub trait Arguments {
    /// A checked upper bound when every field supports contiguous output.
    fn reserve_size(&self) -> Option<usize> {
        None
    }
    /// Encode into a buffer of at least reserve_size bytes.
    fn encode_reserved(&self, _output: &mut [u8]) -> usize {
        panic!("argument sequence has no reserved encoding")
    }
    fn stream<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error>;
    fn append<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> Result<(), S::Error>;
}
impl Arguments for End {
    #[inline]
    fn reserve_size(&self) -> Option<usize> {
        Some(0)
    }
    #[inline]
    fn encode_reserved(&self, _output: &mut [u8]) -> usize {
        0
    }
    #[inline]
    fn stream<S: Sink + ?Sized>(&self, _sink: &mut S) -> Result<(), S::Error> {
        Ok(())
    }
    fn append<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        _batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> Result<(), S::Error> {
        Ok(())
    }
}
impl<H: Encode + ?Sized, T: Arguments> Arguments for Cons<'_, H, T> {
    #[inline]
    fn reserve_size(&self) -> Option<usize> {
        let head = if const { provided(H::borrowed_bytes) } {
            self.head.borrowed_bytes().into_value().len()
        } else if const { provided(H::reserve_size) && provided2(H::encode_reserved) } {
            self.head.reserve_size().into_value()
        } else {
            return None;
        };
        head.checked_add(self.tail.reserve_size()?)
    }
    #[inline]
    fn encode_reserved(&self, output: &mut [u8]) -> usize {
        let used = if const { provided(H::borrowed_bytes) } {
            let bytes = self.head.borrowed_bytes().into_value();
            output[..bytes.len()].copy_from_slice(bytes);
            bytes.len()
        } else if const { provided(H::reserve_size) && provided2(H::encode_reserved) } {
            let bound = self.head.reserve_size().into_value();
            let used = self.head.encode_reserved(&mut output[..bound]).into_value();
            assert!(used <= bound, "encoder exceeded its reserved output bound");
            used
        } else {
            panic!("argument has no reserved encoding")
        };
        used + self.tail.encode_reserved(&mut output[used..])
    }
    #[inline]
    fn stream<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        crate::write(sink, self.head)?;
        self.tail.stream(sink)
    }
    #[inline]
    fn append<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> Result<(), S::Error> {
        batch.push(self.head)?;
        self.tail.append(batch)
    }
}

/// Format an argument sequence using the default bounded batch.
#[doc(hidden)]
#[inline]
pub fn write_arguments<S: Sink + ?Sized, A: Arguments>(
    sink: &mut S,
    arguments: &A,
) -> Result<(), S::Error> {
    if const { crate::provided2(S::write_arguments::<A>) } {
        use crate::Presence;
        return sink.write_arguments(arguments).into_value();
    }
    let mut batch = Batch::<S>::new(sink);
    arguments.append(&mut batch)?;
    batch.finish()
}

#[doc(hidden)]
#[macro_export]
macro_rules! __fast_io_args {
    () => { $crate::arguments::End };
    ($head:expr $(, $tail:expr)* $(,)?) => {
        $crate::arguments::Cons {
            head: &$head,
            tail: $crate::__fast_io_args!($($tail),*)
        }
    };
}

/// Print heterogeneous values to a sink, returning its I/O result.
///
/// Argument expressions are evaluated once, left to right, before output.
/// Temporaries live through the entire operation. No implicit separators or
/// device flush are added.
///
/// ```
/// let mut storage = [0; 64];
/// let mut output = fast_io::SliceSink::new(&mut storage);
/// fast_io::print!(&mut output; "answer=", 42, " ", true).unwrap();
/// assert_eq!(output.as_bytes(), b"answer=42 true");
/// ```
#[macro_export]
macro_rules! print {
    ($sink:expr; $($value:expr),* $(,)?) => {
        $crate::arguments::write_arguments(
            $sink,
            &$crate::__fast_io_args!($($value),*)
        )
    };
}

/// Print values followed by one LF byte; does not flush the device.
#[macro_export]
macro_rules! println {
    ($sink:expr; $($value:expr),* $(,)?) => {
        $crate::print!($sink; $($value,)* "\n")
    };
}
