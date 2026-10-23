//! Composition without intermediate formatted strings.
use crate::{
    Batch, Encode, Presence, Sink,
    arguments::{Arguments, write_arguments},
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
impl<T: Encode> Encode for Option<T> {
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
