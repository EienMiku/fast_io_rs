//! Compile as a library with opt-level=0; see docs/DESIGN.md.
#![no_std]
use fast_io::{Encode, provided};

fn choose<T: Encode + ?Sized>() -> u8 {
    if const { provided(T::borrowed_bytes) } {
        10
    } else {
        20
    }
}

pub fn borrowed() -> u8 {
    choose::<str>()
}
pub fn reserved() -> u8 {
    choose::<u64>()
}
