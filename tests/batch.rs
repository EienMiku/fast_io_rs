#![cfg(feature = "alloc")]
use fast_io::{Batch, Encode, Presence, Sink};

struct Borrow<'a>(&'a str);
impl Encode for Borrow<'_> {
    fn encode<S: Sink + ?Sized>(&self, _: &mut S) -> Result<(), S::Error> {
        panic!("borrowed fast path was not selected")
    }
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.0.as_bytes()
    }
}
struct Reserved;
impl Encode for Reserved {
    fn encode<S: Sink + ?Sized>(&self, _: &mut S) -> Result<(), S::Error> {
        panic!("reserved fast path was not selected")
    }
    fn reserve_size(&self) -> impl Presence<usize> {
        4
    }
    fn encode_reserved(&self, out: &mut [u8]) -> impl Presence<usize> {
        out[..4].copy_from_slice(b"fast");
        4
    }
}
struct Stream;
impl Encode for Stream {
    fn encode<S: Sink + ?Sized>(&self, out: &mut S) -> Result<(), S::Error> {
        out.write_all(b"/")?;
        out.write_all(b"stream")
    }
}
#[derive(Default)]
struct Recorder {
    bytes: Vec<u8>,
    calls: usize,
    pointers: Vec<usize>,
    lengths: Vec<usize>,
}
impl Sink for Recorder {
    type Error = core::convert::Infallible;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.calls += 1;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn write_scatter(&mut self, pieces: &[&[u8]]) -> impl Presence<Result<(), Self::Error>> {
        self.calls += 1;
        for bytes in pieces {
            self.pointers.push(bytes.as_ptr() as usize);
            self.lengths.push(bytes.len());
            self.bytes.extend_from_slice(bytes);
        }
        Ok(())
    }
}
#[test]
fn fast_paths_temporaries_and_scratch_coalescing() {
    let source = String::from("borrowed/");
    let mut sink = Recorder::default();
    fast_io::println!(&mut sink; Borrow(&source), Reserved, 12u8, Stream, String::from("/temporary")).unwrap();
    assert_eq!(sink.bytes, b"borrowed/fast12/stream/temporary\n");
    assert_eq!(sink.calls, 1);
    assert_eq!(sink.pointers[0], source.as_ptr() as usize);
    assert_eq!(sink.lengths, [9, 13, 10, 1]);
}
#[test]
fn tiny_batches_preserve_order_across_automatic_drains() {
    let mut sink = Vec::new();
    let mut batch = Batch::<_, 4, 2>::new(&mut sink);
    batch.push("ab").unwrap();
    batch.push(&123u16).unwrap();
    batch.push("cd").unwrap();
    batch.push("ef").unwrap();
    batch.write_all(b"ghijkl").unwrap();
    batch.push(&'🦀').unwrap();
    batch.finish().unwrap();
    assert_eq!(sink, "ab123cdefghijkl🦀".as_bytes());
}

#[test]
fn compositions_flatten_into_the_outer_batch() {
    fn reserved<T: Encode>(_: &T) -> bool {
        const { fast_io::provided(T::reserve_size) && fast_io::provided2(T::encode_reserved) }
    }
    assert!(reserved(&fast_io::concat!(1u32, "x")));
    assert!(!reserved(&fast_io::concat!(Stream)));
    assert!(reserved(&Some(123u32)));
    assert!(!reserved(&Some(Stream)));
    assert!(reserved(&fast_io::join(&[1u8, 2, 3], ",")));
    assert!(!reserved(&fast_io::join(&[Stream], ",")));
    assert_eq!(
        fast_io::format_bytes!(fast_io::join(&[1u8, 2, 3], ","), Some("!"), None::<u8>),
        b"1,2,3!"
    );
    assert_eq!(
        fast_io::format_bytes!(fast_io::join(&[] as &[u8], ",")),
        b""
    );
    let text = String::from("nested");
    let words = ["one", "two"];
    let mut sink = Recorder::default();
    fast_io::print!(&mut sink;
        "[", fast_io::concat!(&text, ":", 7u32), "] ",
        fast_io::join(&words, "/"), Some("!"), None::<u8>
    )
    .unwrap();
    assert_eq!(sink.bytes, b"[nested:7] one/two!");
    assert_eq!(sink.calls, 1);
    assert!(sink.pointers.contains(&(text.as_ptr() as usize)));
}
