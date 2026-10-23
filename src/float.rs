//! Shortest floats via zmij; explicit precision via core::fmt with reserved output.
use crate::{Encode, Presence, Sink};
use core::fmt;

macro_rules! float {
    ($ty:ty) => {
        impl Encode for $ty {
            #[inline]
            fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
                let mut buffer = zmij::Buffer::new();
                sink.write_all(buffer.format(*self).as_bytes())
            }
            #[inline]
            fn reserve_size(&self) -> impl Presence<usize> {
                32usize
            }
            #[inline]
            fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
                let mut buffer = zmij::Buffer::new();
                let bytes = buffer.format(*self).as_bytes();
                output[..bytes.len()].copy_from_slice(bytes);
                bytes.len()
            }
        }
    };
}
float!(f32);
float!(f64);

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// Primitive floating-point values accepted by explicit-precision formatters.
pub trait Float: sealed::Sealed + Copy + fmt::Display + fmt::LowerExp + fmt::UpperExp {
    #[doc(hidden)]
    const MAX_INTEGER_DIGITS: usize;
    #[doc(hidden)]
    fn integer_digits_bound(self) -> Option<usize>;
}
macro_rules! float_bound {
    ($ty:ty, $mantissa:expr, $mask:expr, $bias:expr, $digits:expr) => {
        impl Float for $ty {
            const MAX_INTEGER_DIGITS: usize = $digits;
            fn integer_digits_bound(self) -> Option<usize> {
                let exponent = ((self.to_bits() >> $mantissa) & $mask) as u32;
                if exponent == $mask {
                    return None;
                }
                // Magnitude is below 2^bits, or below 1 when bits is zero.
                // Rounding to a nonnegative precision cannot exceed 2^bits.
                // 30103/100000 slightly overestimates log10(2).
                let bits = exponent.saturating_sub($bias - 1);
                Some((bits * 30103 / 100000 + 1) as usize)
            }
        }
    };
}
float_bound!(f32, 23, 0xff, 127, 39);
float_bound!(f64, 52, 0x7ff, 1023, 309);

#[derive(Clone, Copy, Debug)]
enum Style {
    Fixed,
    Scientific,
    UpperScientific,
}

/// A fixed-point or scientific floating-point representation.
///
/// Uses core::fmt's rounding and special-value conventions without allocation.
/// The precision is the number of digits after the decimal point. Reserved
/// output uses a conservative bound, including for very large finite values.
#[derive(Clone, Copy, Debug)]
pub struct FloatFormat<T> {
    value: T,
    precision: u16,
    style: Style,
}

/// Format a float with exactly `precision` fractional digits.
/// NaN and infinities retain their usual symbolic representations.
pub fn fixed<T: Float>(value: T, precision: u16) -> FloatFormat<T> {
    FloatFormat {
        value,
        precision,
        style: Style::Fixed,
    }
}

/// Format a float with one leading digit and an explicit decimal exponent.
/// `precision` controls the fractional digits; the exponent uses a lowercase e.
pub fn scientific<T: Float>(value: T, precision: u16) -> FloatFormat<T> {
    FloatFormat {
        value,
        precision,
        style: Style::Scientific,
    }
}

impl<T> FloatFormat<T> {
    /// Use E for a scientific exponent. Has no effect on fixed-point notation
    /// or the spelling of NaN and infinity.
    pub fn uppercase(mut self) -> Self {
        if let Style::Scientific = self.style {
            self.style = Style::UpperScientific;
        }
        self
    }
}

impl<T: Float> fmt::Display for FloatFormat<T> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let precision = usize::from(self.precision);
        match self.style {
            Style::Fixed => write!(output, "{:.*}", precision, self.value),
            Style::Scientific => write!(output, "{:.*e}", precision, self.value),
            Style::UpperScientific => write!(output, "{:.*E}", precision, self.value),
        }
    }
}

impl<T: Float> Encode for FloatFormat<T> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        crate::write_fmt(sink, format_args!("{self}"))
    }

    fn reserve_size(&self) -> impl Presence<usize> {
        let overhead = match self.style {
            Style::Fixed => {
                let Some(digits) = self.value.integer_digits_bound() else {
                    return 4; // NaN, inf, -inf; precision has no effect.
                };
                digits + 1 + usize::from(self.precision != 0)
            }
            // Sign, leading digit, point, e, exponent sign, three exponent digits.
            Style::Scientific | Style::UpperScientific => 8,
        };
        usize::from(self.precision)
            .checked_add(overhead)
            .expect("float representation exceeds the target's address space")
    }

    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        let mut sink = crate::SliceSink::new(output);
        self.encode(&mut sink)
            .expect("float output buffer is too small");
        sink.len()
    }
}
