use fast_io::{ParseError, Scan, ScanError, Scanner, Source};

struct Chunks<'a> {
    bytes: &'a [u8],
    size: usize,
}
impl Source for Chunks<'_> {
    type Error = core::convert::Infallible;
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        Ok(&self.bytes[..self.bytes.len().min(self.size)])
    }
    fn consume(&mut self, count: usize) {
        self.bytes = &self.bytes[count..];
    }
}

#[test]
fn scan_across_boundaries_and_recover_after_bad_tokens() {
    for size in 1..=8 {
        let mut scanner = Scanner::<_, 4>::with_capacity(Chunks {
            bytes: b" \t-128 +255 256 xyz 12345 7",
            size,
        });
        let (a, b) = fast_io::scan!(&mut scanner; i8, u8).unwrap();
        assert_eq!((a, b), (-128, 255));
        assert!(matches!(
            scanner.require::<u8>(),
            Err(ScanError::Parse(ParseError::Overflow))
        ));
        assert!(matches!(
            scanner.require::<u8>(),
            Err(ScanError::Parse(ParseError::InvalidDigit { .. }))
        ));
        assert!(matches!(
            scanner.require::<u8>(),
            Err(ScanError::TokenTooLong { limit: 4 })
        ));
        assert_eq!(scanner.require::<u8>().unwrap(), 7);
        assert_eq!(scanner.scan::<u8>().unwrap(), None);
    }
}

#[test]
fn parsing_bounds_and_utf8() {
    assert_eq!(
        i128::scan_token(b"-170141183460469231731687303715884105728"),
        Ok(i128::MIN)
    );
    assert_eq!(
        u128::scan_token(b"340282366920938463463374607431768211455"),
        Ok(u128::MAX)
    );
    assert_eq!(
        u128::scan_token(b"340282366920938463463374607431768211456"),
        Err(ParseError::Overflow)
    );
    assert!(i8::scan_token(b"-129").is_err());
    assert!(u8::scan_token(b"-1").is_err());
    assert!(i8::scan_token(b"+").is_err());
    let mut scanner = Scanner::new("true 🦀 1.25".as_bytes());
    assert!(scanner.require::<bool>().unwrap());
    assert_eq!(scanner.require::<char>().unwrap(), '🦀');
    assert_eq!(scanner.require::<f32>().unwrap(), 1.25);
}

#[cfg(feature = "std")]
#[test]
fn buffered_input_retries_interrupted_reads() {
    use std::io::{self, Read};
    struct Reader {
        input: &'static [u8],
        interrupt: bool,
    }
    impl Read for Reader {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.interrupt {
                self.interrupt = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = out.len().min(self.input.len());
            out[..n].copy_from_slice(&self.input[..n]);
            self.input = &self.input[n..];
            self.interrupt = true;
            Ok(n)
        }
    }
    let reader = Reader {
        input: b"12345 -9",
        interrupt: true,
    };
    let mut scanner = Scanner::new(fast_io::Input::with_capacity(2, reader));
    assert_eq!(scanner.require::<u32>().unwrap(), 12345);
    assert_eq!(scanner.require::<i8>().unwrap(), -9);
    assert!(scanner.scan::<u8>().unwrap().is_none());
}
