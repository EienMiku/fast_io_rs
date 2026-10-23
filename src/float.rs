//! Shortest round-trip float formatting via the optional zmij dependency.
use crate::{Encode, Presence, Sink};

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
