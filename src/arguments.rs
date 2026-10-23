//! Heterogeneous arguments for the print macros.
use crate::{Batch, Encode, Sink};

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
    fn stream<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error>;
    fn append<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> Result<(), S::Error>;
}
impl Arguments for End {
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
