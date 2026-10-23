//! Composition without intermediate formatted strings.
use crate::{
    Batch, Encode, Presence, Sink,
    arguments::{
        Arguments, Cons, End, ReservedChoice, ReservedSize, ReservedWrite, write_arguments,
    },
};

/// A statically typed collection of borrowed formatting arguments.
///
/// Construct with concat!. Its fields are flattened into the caller's batch.
#[derive(Debug)]
pub struct Parts<A> {
    #[doc(hidden)]
    pub arguments: A,
}
impl<A: Arguments> Encode for Parts<A> {
    fn reserve_size(&self) -> impl Presence<usize> {
        crate::arguments::ReservedSize(&self.arguments)
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        crate::arguments::ReservedWrite(&self.arguments, output)
    }
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        write_arguments(sink, &self.arguments)
    }
    fn encode_batch<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> impl Presence<Result<(), S::Error>> {
        self.arguments.append(batch)
    }
}

/// Build a composable formatting value without allocation.
///
/// This borrows its arguments. Inside print!, temporary arguments remain valid
/// until printing finishes.
#[macro_export]
macro_rules! concat {
    ($($value:expr),* $(,)?) => {
        $crate::Parts { arguments: $crate::__fast_io_args!($($value),*) }
    };
}

/// A borrowed slice formatted with a separator between elements.
#[derive(Clone, Copy, Debug)]
pub struct Join<'a, T> {
    values: &'a [T],
    separator: &'a str,
}
pub fn join<'a, T: Encode>(values: &'a [T], separator: &'a str) -> Join<'a, T> {
    Join { values, separator }
}
impl<T: Encode> Encode for Join<'_, T> {
    fn reserve_size(&self) -> impl Presence<usize> {
        ReservedSize(JoinArguments(self))
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        ReservedWrite(JoinArguments(self), output)
    }
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        for (index, value) in self.values.iter().enumerate() {
            if index != 0 {
                sink.write_all(self.separator.as_bytes())?;
            }
            crate::write(sink, value)?;
        }
        Ok(())
    }
    fn encode_batch<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> impl Presence<Result<(), S::Error>> {
        for (index, value) in self.values.iter().enumerate() {
            if index != 0 {
                batch.push(self.separator)?;
            }
            batch.push(value)?;
        }
        Ok(())
    }
}

struct JoinArguments<'a, 'v, T>(&'a Join<'v, T>);
impl<T: Encode> Arguments for JoinArguments<'_, '_, T> {
    const RESERVABLE: bool = <Cons<'_, T, End> as Arguments>::RESERVABLE;

    fn reserve_size(&self) -> Option<usize> {
        if !Self::RESERVABLE {
            return None;
        }
        let mut size = self
            .0
            .separator
            .len()
            .checked_mul(self.0.values.len().saturating_sub(1))?;
        for value in self.0.values {
            size = size.checked_add(
                Cons {
                    head: value,
                    tail: End,
                }
                .reserve_size()?,
            )?;
        }
        Some(size)
    }
    fn encode_reserved(&self, output: &mut [u8]) -> usize {
        let mut used = 0;
        for (index, value) in self.0.values.iter().enumerate() {
            if index != 0 {
                let end = used + self.0.separator.len();
                output[used..end].copy_from_slice(self.0.separator.as_bytes());
                used = end;
            }
            used += Cons {
                head: value,
                tail: End,
            }
            .encode_reserved(&mut output[used..]);
        }
        used
    }
    fn stream<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        self.0.encode(sink)
    }
    fn append<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> Result<(), S::Error> {
        self.0.encode_batch(batch).into_value()
    }
}

impl<T: Encode> Encode for Option<T> {
    fn reserve_size(&self) -> impl Presence<usize> {
        match self {
            Some(value) => ReservedChoice::Left(ReservedSize(Cons {
                head: value,
                tail: End,
            })),
            None => ReservedChoice::Right(ReservedSize(End)),
        }
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        match self {
            Some(value) => ReservedChoice::Left(ReservedWrite(
                Cons {
                    head: value,
                    tail: End,
                },
                output,
            )),
            None => ReservedChoice::Right(ReservedWrite(End, output)),
        }
    }
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        if let Some(value) = self {
            crate::write(sink, value)?;
        }
        Ok(())
    }
    fn encode_batch<'v, S: Sink + ?Sized, const B: usize, const P: usize>(
        &'v self,
        batch: &mut Batch<'_, 'v, S, B, P>,
    ) -> impl Presence<Result<(), S::Error>> {
        if let Some(value) = self {
            batch.push(value)?;
        }
        Ok(())
    }
}
