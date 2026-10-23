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
    let mut views = Scanner::new("name 42 next".as_bytes());
    let name = views.scan_str().unwrap().unwrap();
    assert_eq!(views.require::<u8>().unwrap(), 42);
    assert_eq!(views.scan_bytes().unwrap(), Some(b"next".as_slice()));
    assert_eq!(name, "name");
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

#[test]
fn binary_endian_values_cross_source_boundaries() {
    let mut bytes = [0; 64];
    let mut output = fast_io::SliceSink::new(&mut bytes);
    fast_io::print!(&mut output;
        fast_io::be(0x1234u16), fast_io::le(i128::MIN), fast_io::be(1.25f64)
    )
    .unwrap();
    let encoded = output.into_written();
    assert_eq!(&encoded[..2], &[0x12, 0x34]);
    for size in 1..=8 {
        let mut source = Chunks {
            bytes: encoded,
            size,
        };
        assert_eq!(
            fast_io::read_be::<u16, _>(&mut source).unwrap(),
            Some(0x1234)
        );
        assert_eq!(
            fast_io::read_le::<i128, _>(&mut source).unwrap(),
            Some(i128::MIN)
        );
        assert_eq!(fast_io::read_be::<f64, _>(&mut source).unwrap(), Some(1.25));
        assert_eq!(fast_io::read_le::<u8, _>(&mut source).unwrap(), None);
    }
    let mut truncated = [1u8].as_slice();
    assert!(matches!(
        fast_io::read_le::<u32, _>(&mut truncated),
        Err(fast_io::ReadError::UnexpectedEof {
            expected: 4,
            actual: 1
        })
    ));
}

#[test]
fn bounded_records_handle_crlf_utf8_and_recovery() {
    use fast_io::{RecordError, read_line, read_record};
    for size in 1..=8 {
        let mut source = Chunks {
            bytes: "\r\n🦀\r\n123456789\nok\nlast\r".as_bytes(),
            size,
        };
        let mut buffer = [0; 8];
        assert_eq!(read_line(&mut source, &mut buffer).unwrap(), Some(""));
        assert_eq!(read_line(&mut source, &mut buffer).unwrap(), Some("🦀"));
        assert!(matches!(
            read_line(&mut source, &mut buffer),
            Err(RecordError::TooLong { limit: 8 })
        ));
        assert_eq!(read_line(&mut source, &mut buffer).unwrap(), Some("ok"));
        assert_eq!(read_line(&mut source, &mut buffer).unwrap(), Some("last\r"));
        assert_eq!(read_line(&mut source, &mut buffer).unwrap(), None);
    }
    let mut source = b"\xff\nvalid\n".as_slice();
    let mut buffer = [0; 8];
    assert!(matches!(
        read_line(&mut source, &mut buffer),
        Err(RecordError::InvalidUtf8)
    ));
    assert_eq!(read_line(&mut source, &mut buffer).unwrap(), Some("valid"));
    let mut source = b",x,,".as_slice();
    assert_eq!(read_record(&mut source, b',', &mut []).unwrap(), Some(0));
    assert!(matches!(
        read_record(&mut source, b',', &mut []),
        Err(RecordError::TooLong { limit: 0 })
    ));
    assert_eq!(read_record(&mut source, b',', &mut []).unwrap(), Some(0));
    assert_eq!(read_record(&mut source, b',', &mut []).unwrap(), None);
}
