//! Byte-width alignment without requiring allocation.
use crate::{Encode, Presence, Sink, provided, provided2};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

/// A byte-width padded value. Width counts bytes, not Unicode display columns.
///
/// Padding never truncates a value. The fill byte may be any byte; output is not
/// necessarily UTF-8. In the streaming fallback, right/center alignment measures
/// non-borrowed values in a first pass, so their Encode implementation must be
/// repeatable. The direct reserved path encodes the value once.
#[derive(Clone, Copy, Debug)]
pub struct Padding<T> {
    value: T,
    width: usize,
    fill: u8,
    alignment: Alignment,
}
pub fn pad<T: Encode>(value: T, width: usize) -> Padding<T> {
    Padding {
        value,
        width,
        fill: b' ',
        alignment: Alignment::Right,
    }
}
impl<T> Padding<T> {
    pub fn left(mut self) -> Self {
        self.alignment = Alignment::Left;
        self
    }
    pub fn right(mut self) -> Self {
        self.alignment = Alignment::Right;
        self
    }
    pub fn center(mut self) -> Self {
        self.alignment = Alignment::Center;
        self
    }
    pub fn fill(mut self, fill: u8) -> Self {
        self.fill = fill;
        self
    }
}
impl<T: Encode> Padding<T> {
    const RESERVABLE: bool =
        provided(T::borrowed_bytes) || (provided(T::reserve_size) && provided2(T::encode_reserved));

    fn bound(&self) -> usize {
        let value = if const { provided(T::borrowed_bytes) } {
            self.value.borrowed_bytes().into_value().len()
        } else {
            self.value.reserve_size().into_value()
        };
        self.width.max(value)
    }

    fn encode_buffer(&self, output: &mut [u8]) -> usize {
        let len = if const { provided(T::borrowed_bytes) } {
            let bytes = self.value.borrowed_bytes().into_value();
            output[..bytes.len()].copy_from_slice(bytes);
            bytes.len()
        } else {
            let bound = self.value.reserve_size().into_value();
            let used = self
                .value
                .encode_reserved(&mut output[..bound])
                .into_value();
            assert!(used <= bound, "encoder exceeded its reserved output bound");
            used
        };
        let total = self.width.max(len);
        let left = self.left_padding(total - len);
        output.copy_within(..len, left);
        output[..left].fill(self.fill);
        output[left + len..total].fill(self.fill);
        total
    }

    fn left_padding(&self, padding: usize) -> usize {
        match self.alignment {
            Alignment::Left => 0,
            Alignment::Right => padding,
            Alignment::Center => padding / 2,
        }
    }
}

struct Size<'a, T>(&'a Padding<T>);
impl<T: Encode> Presence<usize> for Size<'_, T> {
    const PRESENT: bool = Padding::<T>::RESERVABLE;
    fn into_value(self) -> usize {
        self.0.bound()
    }
}
struct Reserved<'a, 'b, T>(&'a Padding<T>, &'b mut [u8]);
impl<T: Encode> Presence<usize> for Reserved<'_, '_, T> {
    const PRESENT: bool = Padding::<T>::RESERVABLE;
    fn into_value(self) -> usize {
        self.0.encode_buffer(self.1)
    }
}

struct Count<S> {
    sink: S,
    bytes: usize,
}
impl<S: Sink> Sink for Count<S> {
    type Error = S::Error;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), S::Error> {
        self.sink.write_all(bytes)?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .expect("output length overflow");
        Ok(())
    }
}
struct Discard;
impl Sink for Discard {
    type Error = core::convert::Infallible;
    fn write_all(&mut self, _: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}
fn fill<S: Sink + ?Sized>(sink: &mut S, byte: u8, mut count: usize) -> Result<(), S::Error> {
    let bytes = [byte; 64];
    while count != 0 {
        let n = count.min(bytes.len());
        sink.write_all(&bytes[..n])?;
        count -= n;
    }
    Ok(())
}
impl<T: Encode> Encode for Padding<T> {
    fn encode<S: Sink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        if self.alignment == Alignment::Left {
            let mut counter = Count {
                sink: &mut *sink,
                bytes: 0,
            };
            crate::write(&mut counter, &self.value)?;
            let padding = self.width.saturating_sub(counter.bytes);
            return fill(sink, self.fill, padding);
        }
        let len = if const { provided(T::borrowed_bytes) } {
            self.value.borrowed_bytes().into_value().len()
        } else {
            let mut counter = Count {
                sink: Discard,
                bytes: 0,
            };
            let Ok(()) = self.value.encode(&mut counter);
            counter.bytes
        };
        let padding = self.width.saturating_sub(len);
        let left = self.left_padding(padding);
        fill(sink, self.fill, left)?;
        crate::write(sink, &self.value)?;
        fill(sink, self.fill, padding - left)
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        Size(self)
    }
    fn encode_reserved(&self, output: &mut [u8]) -> impl Presence<usize> {
        Reserved(self, output)
    }
}
