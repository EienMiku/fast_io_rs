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
