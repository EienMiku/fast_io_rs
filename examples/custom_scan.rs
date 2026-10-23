//! A custom parser opting into chunked scanning without a capability flag.
use fast_io::{BufferedInput, ParseError, Presence, Scan, ScanError, Scanner, Source};

#[derive(Debug, PartialEq)]
struct TokenBytes(usize);

impl Scan for TokenBytes {
    fn scan_token(token: &[u8]) -> Result<Self, ParseError> {
        Ok(Self(token.len()))
    }

    fn scan_stream<S: Source, const N: usize>(
        scanner: &mut Scanner<S, N>,
    ) -> impl Presence<Result<Option<Self>, ScanError<S::Error>>> {
        // Returning the ordinary result enables the hook. scan_fold enforces
        // token boundaries and limits without collecting bytes in a scratch array.
        scanner
            .scan_fold(0usize, |count, chunk| Ok(count + chunk.len()))
            .map(|token| token.map(Self))
    }
}

fn main() -> Result<(), ScanError<core::convert::Infallible>> {
    let input = BufferedInput::<_, 4>::new("hello 🦀rust".as_bytes());
    let mut scanner = Scanner::<_, { usize::MAX }>::with_capacity(input);
    let first = scanner.require::<TokenBytes>()?;
    let second = scanner.require::<TokenBytes>()?;
    assert_eq!(first, TokenBytes(5));
    assert_eq!(second, TokenBytes(8));
    assert_eq!(scanner.scan::<TokenBytes>()?, None);
    println!("token byte lengths: {first:?}, {second:?}");
    Ok(())
}
