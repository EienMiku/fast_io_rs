use fast_io::{BufferedInput, Reader, Scanner, Source};

#[test]
fn fixed_input_handles_short_reads_and_recovers_after_device_errors() {
    struct Device {
        bytes: &'static [u8],
        fail: bool,
    }
    impl Reader for Device {
        type Error = ();
        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ()> {
            if core::mem::take(&mut self.fail) {
                return Err(());
            }
            let len = destination.len().min(2);
            Ok(self.bytes.read(&mut destination[..len]).unwrap())
        }
    }
    let mut source = BufferedInput::<_, 4>::new(Device {
        bytes: b"12345 -7 next",
        fail: true,
    });
    assert!(source.fill_buf().is_err());
    assert_eq!(source.buffer(), b"");
    let mut scanner = Scanner::new(source);
    assert_eq!(scanner.require::<u32>().unwrap(), 12345);
    assert_eq!(scanner.require::<i8>().unwrap(), -7);
    assert_eq!(
        scanner.scan_with(|bytes| Ok(bytes == b"next")).unwrap(),
        Some(true)
    );
    assert_eq!(scanner.scan::<u8>().unwrap(), None);
    assert!(std::panic::catch_unwind(|| BufferedInput::<_, 0>::new(b"data".as_slice())).is_err());
}

#[test]
fn fixed_input_consumes_prefetch_before_bypassing_the_buffer() {
    struct Device {
        bytes: &'static [u8],
        requests: Vec<usize>,
    }
    impl Reader for Device {
        type Error = core::convert::Infallible;
        fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
            self.requests.push(destination.len());
            self.bytes.read(destination)
        }
    }
    let mut source = BufferedInput::<_, 4>::new(Device {
        bytes: b"abcdefghijkl",
        requests: Vec::new(),
    });
    assert_eq!(source.fill_buf().unwrap(), b"abcd");
    assert_eq!(source.fill_buf().unwrap(), b"abcd");
    source.consume(2);
    let mut bytes = [0; 8];
    fast_io::read_exact(&mut source, &mut bytes).unwrap();
    assert_eq!(&bytes, b"cdefghij");
    assert_eq!(source.get_ref().requests, [4, 6]);
    assert_eq!(source.fill_buf().unwrap(), b"kl");
    assert_eq!(source.buffer(), b"kl");
}

#[test]
fn exact_reads_use_cached_bytes_and_respect_limits() {
    struct Cached(&'static [u8]);
    impl Source for Cached {
        type Error = core::convert::Infallible;
        fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
            panic!("already-buffered reads must not enter the refill path");
        }
        fn consume(&mut self, count: usize) {
            self.0 = &self.0[count..];
        }
        fn buffered_bytes(&self) -> impl fast_io::Presence<&[u8]> {
            assert!(
                !self.0.is_empty(),
                "empty destination must not query source"
            );
            self.0
        }
    }
    let mut input = Cached(b"payloadtail");
    let mut limited = fast_io::take(&mut input, 7);
    let mut payload = [0; 7];
    fast_io::read_exact(&mut limited, &mut payload).unwrap();
    assert_eq!(&payload, b"payload");
    assert_eq!(limited.remaining(), 0);
    let mut tail = [0; 4];
    fast_io::read_exact(&mut input, &mut tail).unwrap();
    assert_eq!(&tail, b"tail");
    fast_io::read_exact(&mut input, &mut []).unwrap();

    // An empty cache is not EOF; a partial cache must precede fresh input.
    let mut input = BufferedInput::<_, 4>::new(b"abcde".as_slice());
    fast_io::read_exact(&mut input, &mut payload[..3]).unwrap();
    fast_io::read_exact(&mut input, &mut payload[3..5]).unwrap();
    assert_eq!(&payload[..5], b"abcde");
}

#[cfg(feature = "std")]
#[test]
fn input_buffers_interoperate_with_std_without_losing_prefetched_bytes() {
    use std::io::{self, BufRead, Read};
    fn check(mut input: impl Source<Error = io::Error> + BufRead) {
        let mut text = String::new();
        assert_eq!(BufRead::read_line(&mut input, &mut text).unwrap(), 5);
        assert_eq!(text, "head\n");
        assert_eq!(Scanner::new(&mut input).require::<u8>().unwrap(), 42);
        let mut rest = Vec::new();
        Read::read_to_end(&mut input, &mut rest).unwrap();
        assert_eq!(rest, b"\ntail");
    }
    let bytes = b"head\n42\ntail".as_slice();
    check(fast_io::Input::with_capacity(8, bytes));
    check(BufferedInput::<_, 8>::new(fast_io::Io::new(bytes)));
}
