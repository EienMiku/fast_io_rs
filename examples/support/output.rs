use std::io::{self, BufWriter, Write};

#[derive(Clone, Copy)]
pub enum Mode {
    Standard,
    Buffered,
    StandardUnbuffered,
    Scatter,
}
impl Mode {
    pub const ALL: [Self; 4] = [
        Self::Standard,
        Self::Buffered,
        Self::StandardUnbuffered,
        Self::Scatter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "std BufWriter + writeln!",
            Self::Buffered => "fast_io Buffered + println!",
            Self::StandardUnbuffered => "std unbuffered writeln!",
            Self::Scatter => "fast_io Io scatter + println!",
        }
    }

    pub fn send(self, writer: &mut impl Write, count: u64, payload: &str) -> io::Result<()> {
        match self {
            Self::Standard => {
                let mut output = BufWriter::with_capacity(8192, writer);
                standard(&mut output, count, payload)?;
                output.flush()
            }
            Self::Buffered => {
                use fast_io::Sink;
                let mut output = fast_io::Buffered::<_, 8192>::new(fast_io::Io::new(writer));
                fast(&mut output, count, payload)?;
                output.flush()
            }
            Self::Scatter => fast(&mut fast_io::Io::new(writer), count, payload),
            Self::StandardUnbuffered => standard(writer, count, payload),
        }
    }
}

pub fn standard(output: &mut impl Write, count: u64, payload: &str) -> io::Result<()> {
    for index in 0..count {
        writeln!(output, "id={index} payload={payload} end={}", index % 10)?;
    }
    Ok(())
}

fn fast(
    output: &mut impl fast_io::Sink<Error = io::Error>,
    count: u64,
    payload: &str,
) -> io::Result<()> {
    for index in 0..count {
        fast_io::println!(output; "id=", index, " payload=", payload, " end=", index % 10)?;
    }
    Ok(())
}
