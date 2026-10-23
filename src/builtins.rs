use crate::{Encode, Presence, Sink};

impl Encode for str {
    #[inline]
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self.as_bytes())
    }
    #[inline]
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.as_bytes()
    }
}
impl Encode for [u8] {
    #[inline]
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self)
    }
    #[inline]
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self
    }
}
impl<const N: usize> Encode for [u8; N] {
    #[inline]
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self)
    }
    #[inline]
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.as_slice()
    }
}
#[cfg(feature = "alloc")]
impl Encode for alloc::string::String {
    #[inline]
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self.as_bytes())
    }
    #[inline]
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.as_bytes()
    }
}
#[cfg(feature = "alloc")]
impl Encode for alloc::vec::Vec<u8> {
    #[inline]
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self)
    }
    #[inline]
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.as_slice()
    }
}
impl Encode for bool {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        sink.write_all(self.borrowed_bytes().into_value())
    }
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        if *self {
            b"true".as_slice()
        } else {
            b"false".as_slice()
        }
    }
}
impl Encode for char {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        let mut buffer = [0; 4];
        sink.write_all(self.encode_utf8(&mut buffer).as_bytes())
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        self.len_utf8()
    }
    fn encode_reserved(&self, buffer: &mut [u8]) -> impl Presence<usize> {
        self.encode_utf8(buffer).len()
    }
}
impl Encode for () {
    fn encode<S: Sink + ?Sized>(&self, _sink: &mut S) -> Result<(), S::Error> {
        Ok(())
    }
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        b"".as_slice()
    }
}

const fn digit_pairs() -> [u8; 200] {
    let mut out = [0; 200];
    let mut i = 0;
    while i < 100 {
        out[2 * i] = b'0' + (i / 10) as u8;
        out[2 * i + 1] = b'0' + (i % 10) as u8;
        i += 1;
    }
    out
}
const PAIRS: [u8; 200] = digit_pairs();

macro_rules! decimal {
    ($name:ident, $ty:ty) => {
        #[inline]
        fn $name(mut value: $ty, buffer: &mut [u8]) -> usize {
            let len = if value == 0 {
                1
            } else {
                value.ilog10() as usize + 1
            };
            // Check the whole region once; each iterator item is a complete
            // output pair, including the leading pair for even digit counts.
            let mut pairs = buffer[..len].rchunks_exact_mut(2);
            for output in &mut pairs {
                let pair = (value % 100) as usize;
                value /= 100;
                output.copy_from_slice(&PAIRS[2 * pair..2 * pair + 2]);
            }
            if let [first] = pairs.into_remainder() {
                *first = b'0' + value as u8;
            }
            len
        }
    };
}
decimal!(decimal32, u32);
decimal!(decimal128, u128);

// A complete eight-digit tail can be written with four small integer pairs.
// Splitting large values first avoids a serial chain of 64-bit divisions.
#[inline]
fn decimal_tail(mut value: u32, buffer: &mut [u8]) {
    for output in buffer[..8].rchunks_exact_mut(2) {
        let pair = (value % 100) as usize;
        value /= 100;
        output.copy_from_slice(&PAIRS[2 * pair..2 * pair + 2]);
    }
}

// Keep the width test visible when this is called for narrower integer types.
#[inline(always)]
fn decimal64(value: u64, buffer: &mut [u8]) -> usize {
    if let Ok(small) = u32::try_from(value) {
        return decimal32(small, buffer);
    }
    let upper = value / 100_000_000;
    let lower = (value % 100_000_000) as u32;
    let len = if upper < 100_000_000 {
        decimal32(upper as u32, buffer)
    } else {
        let len = decimal32((upper / 100_000_000) as u32, buffer);
        decimal_tail((upper % 100_000_000) as u32, &mut buffer[len..]);
        len + 8
    };
    decimal_tail(lower, &mut buffer[len..]);
    len + 8
}

macro_rules! unsigned {
    ($ty:ty, $wide:ty, $decimal:ident) => {
        impl Encode for $ty {
            #[inline]
            fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
                let mut buffer = [0; 40];
                let used = $decimal(*self as $wide, &mut buffer);
                sink.write_all(&buffer[..used])
            }
            #[inline]
            fn reserve_size(&self) -> impl Presence<usize> {
                (<$ty>::BITS as usize * 30103 / 100000) + 1
            }
            #[inline]
            fn encode_reserved(&self, buffer: &mut [u8]) -> impl Presence<usize> {
                $decimal(*self as $wide, buffer)
            }
        }
    };
}
macro_rules! signed {
    ($ty:ty, $wide:ty, $decimal:ident) => {
        impl Encode for $ty {
            #[inline]
            fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
                let mut buffer = [0; 40];
                let used = self.encode_reserved(&mut buffer).into_value();
                sink.write_all(&buffer[..used])
            }
            #[inline]
            fn reserve_size(&self) -> impl Presence<usize> {
                (<$ty>::BITS as usize * 30103 / 100000) + 2
            }
            #[inline]
            fn encode_reserved(&self, buffer: &mut [u8]) -> impl Presence<usize> {
                let negative = (*self < 0) as usize;
                if negative != 0 {
                    buffer[0] = b'-';
                }
                negative + $decimal(self.unsigned_abs() as $wide, &mut buffer[negative..])
            }
        }
    };
}
unsigned!(u8, u64, decimal64);
unsigned!(u16, u64, decimal64);
unsigned!(u32, u64, decimal64);
unsigned!(u64, u64, decimal64);
unsigned!(usize, u64, decimal64);
unsigned!(u128, u128, decimal128);
signed!(i8, u64, decimal64);
signed!(i16, u64, decimal64);
signed!(i32, u64, decimal64);
signed!(i64, u64, decimal64);
signed!(isize, u64, decimal64);
signed!(i128, u128, decimal128);
