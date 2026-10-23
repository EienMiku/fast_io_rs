#![cfg_attr(not(feature = "std"), no_std)]
//! Composable static-dispatch I/O and formatting, inspired by C++ fast_io.
#[cfg(feature = "alloc")]
extern crate alloc;

#[doc(hidden)]
pub mod arguments;
pub mod batch;
pub mod buffered;
mod builtins;
pub mod fmt_bridge;
pub mod memory;
#[cfg(feature = "std")]
pub mod std_io;

pub mod encode;
pub mod presence;
pub mod sink;

pub use batch::Batch;
pub use buffered::Buffered;
pub use encode::{Encode, write};
pub use fmt_bridge::{debug, display, write_fmt};
pub use memory::{CapacityError, SliceSink};
pub use presence::{Presence, provided, provided0, provided2, provided3};
pub use sink::{Sink, write_scatter};
#[cfg(feature = "std")]
pub use std_io::Io;
