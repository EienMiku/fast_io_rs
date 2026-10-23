//! Interoperability with core::fmt formatters.
use crate::{Encode, Sink};
use core::fmt;

struct Writer<'a, S: Sink + ?Sized> {
    sink: &'a mut S,
    error: Option<S::Error>,
}
impl<S: Sink + ?Sized> fmt::Write for Writer<'_, S> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.error.is_some() {
            return Err(fmt::Error);
        }
        match self.sink.write_all(text.as_bytes()) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.error = Some(error);
                Err(fmt::Error)
            }
        }
    }
}

/// Write standard formatting arguments to a sink without allocating.
///
/// Panics if a user formatter returns fmt::Error without a sink error, violating
/// the core::fmt formatting contract.
pub fn write_fmt<S: Sink + ?Sized>(sink: &mut S, args: fmt::Arguments<'_>) -> Result<(), S::Error> {
    let mut writer = Writer { sink, error: None };
    let result = fmt::write(&mut writer, args);
    // Preserve errors even if a custom formatter incorrectly swallows fmt::Error.
    if let Some(error) = writer.error {
        return Err(error);
    }
    result.expect("formatter returned fmt::Error without an output error");
    Ok(())
}

/// Adapt an existing Display implementation to Encode.
#[derive(Clone, Copy, Debug)]
pub struct Display<T>(pub T);
pub fn display<T: fmt::Display>(value: T) -> Display<T> {
    Display(value)
}
impl<T: fmt::Display> Encode for Display<T> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        write_fmt(sink, format_args!("{}", self.0))
    }
}

/// Adapt an existing Debug implementation to Encode.
#[derive(Clone, Copy, Debug)]
pub struct Debug<T>(pub T);
pub fn debug<T: fmt::Debug>(value: T) -> Debug<T> {
    Debug(value)
}
impl<T: fmt::Debug> Encode for Debug<T> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        write_fmt(sink, format_args!("{:?}", self.0))
    }
}
impl Encode for fmt::Arguments<'_> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        write_fmt(sink, *self)
    }
}
