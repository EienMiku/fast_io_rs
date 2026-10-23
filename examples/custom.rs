use fast_io::{Encode, Io, Presence, Sink};

/// A borrowed representation only needs the normal fallback and its one hook.
struct Message<'a>(&'a str);
impl Encode for Message<'_> {
    fn encode<S: Sink + ?Sized>(&self, out: &mut S) -> Result<(), S::Error> {
        out.write_all(self.0.as_bytes())
    }
    fn borrowed_bytes(&self) -> impl Presence<&[u8]> {
        self.0.as_bytes()
    }
}

/// The derive generates streaming, shared-batch and conditional reserve paths.
#[derive(Encode)]
#[encode("[{sequence}] {message}")]
struct Record<'a> {
    sequence: u64,
    message: Message<'a>,
}

fn main() -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = Io::new(stdout.lock());
    let record = Record {
        sequence: 42,
        message: Message("hello from a custom formatter"),
    };
    fast_io::println!(&mut out; record)?;
    fast_io::println!(&mut out;
        "reserve capability: ", const { fast_io::provided(Record::reserve_size) }
    )?;
    out.flush()
}
