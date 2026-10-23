//! Unsigned integer formatting in bases 2 through 36.
use crate::{Encode, Presence, Sink};

mod sealed {
    pub trait Sealed {}
}
/// Primitive unsigned integers accepted by radix formatters.
pub trait Unsigned: sealed::Sealed {
    #[doc(hidden)]
    fn into_u128(self) -> u128;
}
macro_rules! unsigned {
    ($($t:ty),*) => {$(
        impl sealed::Sealed for $t {}
        impl Unsigned for $t { fn into_u128(self) -> u128 { self as u128 } }
    )*};
}
unsigned!(u8, u16, u32, u64, u128, usize);

/// A radix formatter. Prefixes are available for binary, octal and hexadecimal.
#[derive(Clone, Copy, Debug)]
pub struct Radix {
    value: u128,
    base: u8,
    uppercase: bool,
    prefix: bool,
}
pub fn radix<T: Unsigned>(value: T, base: u8) -> Radix {
    assert!((2..=36).contains(&base), "radix must be in 2..=36");
    Radix {
        value: value.into_u128(),
        base,
        uppercase: false,
        prefix: false,
    }
}
pub fn hex<T: Unsigned>(value: T) -> Radix {
    radix(value, 16)
}
pub fn bin<T: Unsigned>(value: T) -> Radix {
    radix(value, 2)
}
pub fn oct<T: Unsigned>(value: T) -> Radix {
    radix(value, 8)
}
impl Radix {
    pub fn uppercase(mut self) -> Self {
        self.uppercase = true;
        self
    }
    pub fn prefixed(mut self) -> Self {
        self.prefix = true;
        self
    }
    fn prefix(&self) -> &'static [u8] {
        if !self.prefix {
            return b"";
        }
        match (self.base, self.uppercase) {
            (2, false) => b"0b",
            (2, true) => b"0B",
            (8, false) => b"0o",
            (8, true) => b"0O",
            (16, false) => b"0x",
            (16, true) => b"0X",
            _ => b"",
        }
    }
    fn digits(&self) -> usize {
        if self.base.is_power_of_two() {
            return (128 - self.value.leading_zeros())
                .max(1)
                .div_ceil(self.base.trailing_zeros()) as usize;
        }
        let mut value = self.value;
        let mut len = 1;
        while value >= self.base as u128 {
            value /= self.base as u128;
            len += 1;
        }
        len
    }
}
impl Encode for Radix {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        let mut buffer = [0; 130];
        let used = self.encode_reserved(&mut buffer).into_value();
        sink.write_all(&buffer[..used])
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        self.prefix().len() + self.digits()
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        let prefix = self.prefix();
        output[..prefix.len()].copy_from_slice(prefix);
        let len = self.digits() + prefix.len();
        let alphabet = if self.uppercase {
            b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        } else {
            b"0123456789abcdefghijklmnopqrstuvwxyz"
        };
        let mut value = self.value;
        for position in (prefix.len()..len).rev() {
            output[position] = alphabet[(value % self.base as u128) as usize];
            value /= self.base as u128;
        }
        len
    }
}
