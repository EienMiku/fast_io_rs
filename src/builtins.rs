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
            let mut cursor = len;
            while value >= 100 {
                let pair = (value % 100) as usize;
                value /= 100;
                cursor -= 2;
                buffer[cursor..cursor + 2].copy_from_slice(&PAIRS[2 * pair..2 * pair + 2]);
            }
            if value < 10 {
                buffer[0] = b'0' + value as u8;
            } else {
                let pair = value as usize;
                buffer[..2].copy_from_slice(&PAIRS[2 * pair..2 * pair + 2]);
            }
            len
        }
    };
}
decimal!(decimal64, u64);
decimal!(decimal128, u128);

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
