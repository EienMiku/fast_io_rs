#![cfg_attr(not(feature = "std"), no_std)]
//! Composable static-dispatch I/O and formatting, inspired by C++ fast_io.
#[cfg(feature = "alloc")]
extern crate alloc;

pub mod encode;
pub mod presence;
pub mod sink;

pub use encode::{Encode, write};
pub use presence::{Presence, provided, provided0, provided2, provided3};
pub use sink::{Sink, write_scatter};
