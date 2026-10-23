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
