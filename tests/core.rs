#![cfg(feature = "alloc")]
use fast_io::{CapacityError, Presence, Sink, SliceSink, write, write_scatter};

#[test]
fn decimal_extremes_and_utf8() {
    for n in [
        i128::MIN,
        i64::MIN as i128,
        -100,
        -1,
        0,
        9,
        10,
        99,
        100,
        i128::MAX,
    ] {
        let mut out = Vec::new();
        write(&mut out, &n).unwrap();
        assert_eq!(out, n.to_string().as_bytes());
    }
    for n in [0u128, 9, 10, 99, 100, u64::MAX as u128, u128::MAX] {
        let mut out = Vec::new();
        write(&mut out, &n).unwrap();
        assert_eq!(out, n.to_string().as_bytes());
    }
    let mut out = Vec::new();
    write(&mut out, "Rust / 中文 / ").unwrap();
    write(&mut out, &'🦀').unwrap();
    write(&mut out, &true).unwrap();
    assert_eq!(out, "Rust / 中文 / 🦀true".as_bytes());
}

#[test]
fn fixed_buffer_rejects_whole_scatter_before_writing() {
    let mut storage = [0; 6];
    let mut out = SliceSink::new(&mut storage);
    out.write_all(b"ok").unwrap();
    let error = out
        .write_scatter(&[b"123", b"45"])
        .into_value()
        .unwrap_err();
    assert_eq!(
        error,
        CapacityError {
            required: 5,
            remaining: 4
        }
    );
    assert_eq!(out.as_bytes(), b"ok");
}

#[cfg(feature = "std")]
#[test]
fn scatter_handles_partial_writes_and_interruptions() {
    use std::io::{self, IoSlice, Write};
    #[derive(Default)]
    struct Partial {
        bytes: Vec<u8>,
        calls: usize,
    }
    impl Write for Partial {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls % 3 == 1 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = buf.len().min(2);
            self.bytes.extend_from_slice(&buf[..n]);
            Ok(n)
        }
        fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
            self.write(bufs.iter().find(|b| !b.is_empty()).map_or(&[], |b| &**b))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut sink = fast_io::Io::new(Partial::default());
    let pieces = [b"".as_slice(), b"abc", b"", b"defgh"];
    write_scatter(&mut sink, &pieces).unwrap();
    assert_eq!(sink.get_ref().bytes, b"abcdefgh");
    let many = [b"x".as_slice(); 37];
    write_scatter(&mut sink, &many).unwrap();
    assert_eq!(&sink.get_ref().bytes[8..], &[b'x'; 37]);
}

#[test]
fn persistent_buffering_and_fmt_bridge() {
    let mut output = fast_io::Buffered::<_, 8>::new(Vec::new());
    fast_io::print!(&mut output; "a=", 12, ";").unwrap();
    assert_eq!(output.pending(), b"a=12;");
    assert!(output.get_ref().is_empty());
    fast_io::print!(&mut output; fast_io::display("text"), fast_io::debug([1, 2])).unwrap();
    assert_eq!(output.finish().unwrap(), b"a=12;text[1, 2]");

    let mut bytes = Vec::new();
    {
        let mut buffered = fast_io::Buffered::<_, 8>::new(&mut bytes);
        buffered.write_all(b"drop").unwrap();
    }
    assert_eq!(bytes, b"drop");
    let mut storage = [];
    let mut full = SliceSink::new(&mut storage);
    assert!(write(&mut full, &fast_io::display("no room")).is_err());
}

#[test]
fn buffered_failure_does_not_replay_a_partial_prefix() {
    struct Fails {
        calls: usize,
    }
    impl Sink for Fails {
        type Error = ();
        fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
            self.calls += 1;
            Err(())
        }
    }
    let mut inner = Fails { calls: 0 };
    {
        let mut buffered = fast_io::Buffered::<_, 8>::new(&mut inner);
        buffered.write_all(b"pending").unwrap();
        assert!(buffered.flush().is_err());
        assert!(buffered.pending().is_empty());
    }
    assert_eq!(inner.calls, 1);

    struct Panics(usize);
    impl Sink for Panics {
        type Error = ();
        fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
            self.0 += 1;
            panic!("sink wrote a prefix then panicked");
        }
    }
    let mut inner = Panics(0);
    let mut batch = fast_io::Batch::<_, 8, 2>::new(&mut inner);
    batch.push("pending").unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| batch.drain())).is_err());
    batch.finish().unwrap();
    assert_eq!(inner.0, 1);
}

#[test]
fn reserved_memory_output_handles_small_buffers_and_unwinding() {
    use fast_io::Encode;
    let mut storage = [0; 1];
    let mut small = SliceSink::new(&mut storage);
    fast_io::print!(&mut small; 7u64).unwrap();
    assert_eq!(small.as_bytes(), b"7");

    struct Panic;
    impl Encode for Panic {
        fn encode<S: Sink + ?Sized>(&self, _: &mut S) -> Result<(), S::Error> {
            panic!("format failure")
        }
        fn reserve_size(&self) -> impl Presence<usize> {
            1
        }
        fn encode_reserved(&self, out: &mut [u8]) -> impl Presence<usize> {
            out[0] = b'x';
            panic!("format failure");
            #[allow(unreachable_code)]
            1usize
        }
    }
    let mut bytes = b"prefix".to_vec();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fast_io::print!(&mut bytes; "uncommitted", Panic).unwrap();
    }));
    assert!(result.is_err());
    assert_eq!(bytes, b"prefix");
}
