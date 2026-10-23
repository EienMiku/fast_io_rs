#![cfg_attr(not(feature = "std"), no_std)]
//! Composable static-dispatch I/O and formatting, inspired by C++ fast_io.
#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "derive")]
pub use fast_io_derive::Encode;

#[doc(hidden)]
pub mod arguments;
pub mod batch;
pub mod binary;
pub mod buffered;
mod builtins;
pub mod compose;
#[cfg(feature = "float")]
mod float;
pub mod fmt_bridge;
pub mod memory;
pub mod padding;
pub mod quoted;
pub mod radix;
pub mod record;
pub mod scan;
pub mod source;
#[cfg(feature = "std")]
pub mod std_input;
#[cfg(feature = "std")]
pub mod std_io;
pub mod transcode;

pub mod encode;
pub mod presence;
pub mod sink;

pub use batch::Batch;
pub use binary::{Binary, Endian, ReadError, be, le, read_be, read_le};
pub use buffered::Buffered;
pub use compose::{Parts, join};
pub use encode::{Encode, write};
pub use fmt_bridge::{debug, display, write_fmt};
pub use memory::{CapacityError, SliceSink};
pub use padding::{Alignment, Padding, pad};
pub use presence::{Presence, provided, provided0, provided2, provided3};
pub use quoted::{Quoted, quoted};
pub use radix::{Radix, bin, hex, oct, radix};
pub use record::{RecordError, read_line, read_record};
pub use scan::{ParseError, Scan, ScanError, Scanner};
pub use sink::{Sink, write_scatter};
pub use source::Source;
#[cfg(feature = "std")]
pub use std_input::Input;
#[cfg(feature = "std")]
pub use std_io::Io;
pub use transcode::{
    Utf16, Utf16Bytes, Utf16Error, Utf16Lossy, utf16, utf16_be, utf16_le, utf16_lossy,
};
