use fast_io::{ParseError, Scan, ScanError, ScanRadix, Scanner, Source};

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
    assert_eq!(u8::scan_radix(b"fF", 16), Ok(255));
    assert_eq!(i8::scan_radix(b"-80", 16), Ok(-128));
    assert_eq!(i8::scan_radix(b"-81", 16), Err(ParseError::Overflow));
    assert_eq!(
        u8::scan_radix(b"fg", 16),
        Err(ParseError::InvalidDigit { offset: 1 })
    );
    let mut bases = Scanner::new(b"101".as_slice());
    assert!(bases.scan_radix::<u8>(1).is_err());
    assert_eq!(bases.scan_radix::<u8>(2).unwrap(), Some(5));
    let mut customized = Scanner::new(b"yes".as_slice());
    let expected = String::from("yes");
    assert_eq!(
        customized
            .scan_with(move |token| {
                // A FnOnce parser can consume its captured configuration.
                let bytes = expected.into_bytes();
                Ok(token == bytes)
            })
            .unwrap(),
        Some(true)
    );
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

#[test]
fn incremental_tokens_fold_without_scratch_and_recover_after_errors() {
    for size in 1..=8 {
        let mut scanner = Scanner::<_, 8>::with_capacity(Chunks {
            bytes: "  🦀ab badtoolong bad ok".as_bytes(),
            size,
        });
        let bytes = scanner
            .scan_fold(Vec::new(), |mut out, chunk| {
                out.extend_from_slice(chunk);
                Ok(out)
            })
            .unwrap()
            .unwrap();
        assert_eq!(bytes, "🦀ab".as_bytes());
        assert!(matches!(
            scanner.scan_fold((), |(), _| Err(ParseError::InvalidValue)),
            Err(ScanError::TokenTooLong { limit: 8 })
        ));
        let mut calls = 0;
        assert!(matches!(
            scanner.scan_fold((), |(), _| {
                calls += 1;
                Err(ParseError::InvalidValue)
            }),
            Err(ScanError::Parse(ParseError::InvalidValue))
        ));
        assert_eq!(calls, 1);
        assert_eq!(
            scanner.scan_fold(0, |n, part| Ok(n + part.len())).unwrap(),
            Some(2)
        );
        assert_eq!(
            scanner
                .scan_fold((), |(), _| panic!("callback at EOF"))
                .unwrap(),
            None
        );
    }
    // An enormous type-level limit does not become a stack allocation here.
    let mut scanner = Scanner::<_, { usize::MAX }>::with_capacity(b"payload".as_slice());
    assert_eq!(
        scanner.scan_fold(0, |n, part| Ok(n + part.len())).unwrap(),
        Some(7)
    );
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

    // Retain prefetched bytes when switching an existing std reader to fast_io.
    use std::io::BufRead;
    let mut buffered = std::io::BufReader::with_capacity(8, b"skip 42".as_slice());
    assert_eq!(buffered.fill_buf().unwrap(), b"skip 42");
    buffered.consume(5);
    let mut scanner = Scanner::new(fast_io::Input::from_buffered(buffered));
    assert_eq!(scanner.require::<u8>().unwrap(), 42);
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
fn raw_reads_fill_across_buffers_and_preserve_unread_data() {
    use fast_io::{ReadError, read, read_exact};
    for size in 1..=8 {
        let mut input = Chunks {
            bytes: b"headpayloadtail",
            size,
        };
        let mut header = [0; 4];
        read_exact(&mut input, &mut header).unwrap();
        assert_eq!(&header, b"head");
        let mut payload = [0; 7];
        read_exact(&mut input, &mut payload).unwrap();
        assert_eq!(&payload, b"payload");
        assert_eq!(input.bytes, b"tail");
        let mut too_large = [b'?'; 6];
        assert!(matches!(
            read_exact(&mut input, &mut too_large),
            Err(ReadError::UnexpectedEof {
                expected: 6,
                actual: 4
            })
        ));
        assert_eq!(&too_large, b"tail??");
        assert_eq!(read(&mut input, &mut too_large).unwrap(), 0);
    }
    struct Fails;
    impl Source for Fails {
        type Error = ();
        fn fill_buf(&mut self) -> Result<&[u8], ()> {
            Err(())
        }
        fn consume(&mut self, _: usize) {
            panic!("failed read must not consume");
        }
    }
    assert_eq!(read(&mut Fails, &mut []).unwrap(), 0);
    read_exact(&mut Fails, &mut []).unwrap();
    assert!(matches!(
        read_exact(&mut Fails, &mut [0]),
        Err(ReadError::Source(()))
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

#[test]
fn stream_copy_respects_limits_and_failed_output() {
    use fast_io::{CopyError, Sink, SliceSink, copy, take};
    for size in 1..=8 {
        let mut source = Chunks {
            bytes: b"payloadtail",
            size,
        };
        let mut bytes = [0; 7];
        let mut output = SliceSink::new(&mut bytes);
        let mut limited = take(&mut source, 7);
        assert_eq!(copy(&mut limited, &mut output).unwrap(), 7);
        assert_eq!(limited.remaining(), 0);
        assert_eq!(output.as_bytes(), b"payload");
        assert_eq!(source.bytes, b"tail");
    }
    struct FailingSink;
    impl Sink for FailingSink {
        type Error = ();
        fn write_all(&mut self, _: &[u8]) -> Result<(), ()> {
            Err(())
        }
    }
    let mut source = b"keep".as_slice();
    assert!(matches!(
        copy(&mut source, &mut FailingSink),
        Err(CopyError::Sink(()))
    ));
    assert_eq!(source, b"keep");
    assert_eq!(
        copy(&mut take(&mut source, 0), &mut FailingSink).unwrap(),
        0
    );
}
