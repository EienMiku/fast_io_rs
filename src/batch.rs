//! Bounded scratch storage and scatter planning.
use crate::{Encode, Presence, Sink, provided, provided2, write_scatter};

#[derive(Clone, Copy, Debug)]
enum Piece<'a> {
    Borrowed(&'a [u8]),
    Scratch { start: usize, end: usize },
}
const EMPTY: Piece<'static> = Piece::Borrowed(&[]);

/// Batch formatting with bounded stack storage and no allocation.
///
/// Borrowed representations are retained until the batch drains; generated
/// bytes live in scratch storage. Adjacent scratch pieces are coalesced.
/// Storage exhaustion drains automatically. `finish` must be called: dropping
/// a batch discards pending bytes and performs no I/O.
///
/// On an I/O error, pending state is discarded to avoid replaying a partially
/// written prefix. The underlying sink may have consumed part of that batch.
#[derive(Debug)]
#[must_use = "call finish to emit pending output"]
pub struct Batch<'s, 'v, S: Sink + ?Sized, const BYTES: usize = 1024, const PIECES: usize = 32> {
    sink: &'s mut S,
    scratch: [u8; BYTES],
    used: usize,
    pieces: [Piece<'v>; PIECES],
    len: usize,
}

impl<'s, 'v, S: Sink + ?Sized, const B: usize, const P: usize> Batch<'s, 'v, S, B, P> {
    pub fn new(sink: &'s mut S) -> Self {
        assert!(P > 0, "a batch requires at least one fragment slot");
        Self {
            sink,
            scratch: [0; B],
            used: 0,
            pieces: [EMPTY; P],
            len: 0,
        }
    }

    /// Append a value, retaining a borrow if its hook supplies one.
    #[inline]
    pub fn push<T: Encode + ?Sized>(&mut self, value: &'v T) -> Result<(), S::Error> {
        if const { provided(T::borrowed_bytes) } {
            let bytes = value.borrowed_bytes().into_value();
            if !bytes.is_empty() {
                if self.len == P {
                    self.drain()?;
                }
                self.pieces[self.len] = Piece::Borrowed(bytes);
                self.len += 1;
            }
            Ok(())
        } else if const { provided2(T::encode_batch::<S, B, P>) } {
            value.encode_batch(self).into_value()
        } else if const { provided(T::reserve_size) && provided2(T::encode_reserved) } {
            let size = value.reserve_size().into_value();
            if size > B {
                return value.encode(self);
            }
            self.prepare_scratch(size)?;
            let start = self.used;
            let buffer = &mut self.scratch[start..start + size];
            let used = value.encode_reserved(buffer).into_value();
            assert!(used <= size, "encoder exceeded its reserved output bound");
            self.used += used;
            self.append_scratch(start);
            Ok(())
        } else {
            value.encode(self)
        }
    }

    fn prepare_scratch(&mut self, size: usize) -> Result<(), S::Error> {
        let can_merge = self.len != 0
            && matches!(self.pieces[self.len - 1], Piece::Scratch { end, .. } if end == self.used);
        if size > B - self.used || (self.len == P && !can_merge) {
            self.drain()?;
        }
        Ok(())
    }

    fn append_scratch(&mut self, start: usize) {
        if start == self.used {
            return;
        }
        if self.len != 0 {
            if let Piece::Scratch { end, .. } = &mut self.pieces[self.len - 1] {
                if *end == start {
                    *end = self.used;
                    return;
                }
            }
        }
        self.pieces[self.len] = Piece::Scratch {
            start,
            end: self.used,
        };
        self.len += 1;
    }

    /// Emit pending bytes without flushing the underlying device.
    pub fn drain(&mut self) -> Result<(), S::Error> {
        if self.len == 0 {
            return Ok(());
        }
        let mut fragments = [&[][..]; P];
        for (fragment, piece) in fragments.iter_mut().zip(&self.pieces[..self.len]) {
            *fragment = match *piece {
                Piece::Borrowed(bytes) => bytes,
                Piece::Scratch { start, end } => &self.scratch[start..end],
            };
        }
        // Clear before calling user code: a sink may write a prefix then return
        // an error or unwind. Catching that panic must not replay this batch.
        let len = core::mem::take(&mut self.len);
        self.pieces[..len].fill(EMPTY);
        self.used = 0;
        write_scatter(self.sink, &fragments[..len])
    }

    /// Emit pending bytes. Does not call the underlying device's flush method.
    pub fn finish(mut self) -> Result<(), S::Error> {
        self.drain()
    }
}

impl<S: Sink + ?Sized, const B: usize, const P: usize> Sink for Batch<'_, '_, S, B, P> {
    type Error = S::Error;

    /// Copy transient bytes into scratch storage, preserving order with borrows.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        if bytes.len() > B {
            self.drain()?;
            return self.sink.write_all(bytes);
        }
        self.prepare_scratch(bytes.len())?;
        let start = self.used;
        self.scratch[start..start + bytes.len()].copy_from_slice(bytes);
        self.used += bytes.len();
        self.append_scratch(start);
        Ok(())
    }

    fn write_scatter(&mut self, fragments: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        for bytes in fragments {
            self.write_all(bytes)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.drain()?;
        self.sink.flush()
    }
}
