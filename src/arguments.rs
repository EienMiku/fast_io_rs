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
    const RESERVABLE: bool = false;
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
impl<A: Arguments + ?Sized> Arguments for &A {
    const RESERVABLE: bool = A::RESERVABLE;
    fn reserve_size(&self) -> Option<usize> {
        A::reserve_size(self)
    }
    fn encode_reserved(&self, output: &mut [u8]) -> usize {
        A::encode_reserved(self, output)
    }
    fn stream<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        A::stream(self, sink)
    }
    fn append<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> Result<(), S::Error> {
        A::append(self, batch)
    }
}
impl Arguments for End {
    const RESERVABLE: bool = true;
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
    const RESERVABLE: bool = (provided(H::borrowed_bytes)
        || (provided(H::reserve_size) && provided2(H::encode_reserved)))
        && T::RESERVABLE;
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

/// A capability-carrying size query for a heterogeneous argument list.
#[doc(hidden)]
#[derive(Debug)]
pub struct ReservedSize<A>(pub A);
impl<A: Arguments> Presence<usize> for ReservedSize<A> {
    const PRESENT: bool = A::RESERVABLE;
    fn into_value(self) -> usize {
        self.0
            .reserve_size()
            .expect("argument list has no finite reserved size")
    }
}

/// A deferred reserved encoding, carrying the same capability as its fields.
#[doc(hidden)]
#[derive(Debug)]
pub struct ReservedWrite<'a, A>(pub A, pub &'a mut [u8]);
impl<A: Arguments> Presence<usize> for ReservedWrite<'_, A> {
    const PRESENT: bool = A::RESERVABLE;
    fn into_value(self) -> usize {
        self.0.encode_reserved(self.1)
    }
}

/// A derived enum's alternatives, present only if every branch is present.
#[doc(hidden)]
#[derive(Debug)]
pub enum ReservedChoice<L, R> {
    Left(L),
    Right(R),
}
impl<L: Presence<usize>, R: Presence<usize>> Presence<usize> for ReservedChoice<L, R> {
    const PRESENT: bool = L::PRESENT && R::PRESENT;
    fn into_value(self) -> usize {
        match self {
            Self::Left(value) => value.into_value(),
            Self::Right(value) => value.into_value(),
        }
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

#[cfg(feature = "std")]
#[doc(hidden)]
pub fn print_stdout<A: Arguments>(arguments: &A) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    write_arguments(&mut crate::Io::new(stdout.lock()), arguments)
}

#[cfg(feature = "std")]
#[doc(hidden)]
pub fn print_stderr<A: Arguments>(arguments: &A) -> std::io::Result<()> {
    let stderr = std::io::stderr();
    write_arguments(&mut crate::Io::new(stderr.lock()), arguments)
}

#[cfg(feature = "alloc")]
#[doc(hidden)]
pub fn format_bytes<A: Arguments>(arguments: &A) -> alloc::vec::Vec<u8> {
    let mut bytes = alloc::vec::Vec::new();
    let Ok(()) = write_arguments(&mut bytes, arguments);
    bytes
}

#[cfg(feature = "alloc")]
#[doc(hidden)]
pub fn format_text<A: Arguments>(
    arguments: &A,
) -> Result<alloc::string::String, alloc::string::FromUtf8Error> {
    alloc::string::String::from_utf8(format_bytes(arguments))
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
    ($($value:expr),* $(,)?) => {
        $crate::arguments::print_stdout(&$crate::__fast_io_args!($($value),*))
    };
}

/// Print values followed by one LF byte; does not flush the device.
#[macro_export]
macro_rules! println {
    ($sink:expr; $($value:expr),* $(,)?) => {
        $crate::print!($sink; $($value,)* "\n")
    };
    ($($value:expr),* $(,)?) => {
        $crate::print!($($value,)* "\n")
    };
}

/// Print to locked stderr, returning any output error. Requires std.
#[cfg(feature = "std")]
#[macro_export]
macro_rules! eprint {
    ($($value:expr),* $(,)?) => {
        $crate::arguments::print_stderr(&$crate::__fast_io_args!($($value),*))
    };
}

/// Print to locked stderr followed by LF. Requires std.
#[cfg(feature = "std")]
#[macro_export]
macro_rules! eprintln {
    ($($value:expr),* $(,)?) => { $crate::eprint!($($value,)* "\n") };
}

/// Format into an owned byte vector. Requires alloc.
#[cfg(feature = "alloc")]
#[macro_export]
macro_rules! format_bytes {
    ($($value:expr),* $(,)?) => {
        $crate::arguments::format_bytes(&$crate::__fast_io_args!($($value),*))
    };
}

/// Format into a UTF-8 String, returning an error for invalid raw bytes.
/// Requires alloc. Byte-oriented formatters may legitimately produce non-UTF-8.
#[cfg(feature = "alloc")]
#[macro_export]
macro_rules! format_text {
    ($($value:expr),* $(,)?) => {
        $crate::arguments::format_text(&$crate::__fast_io_args!($($value),*))
    };
}
