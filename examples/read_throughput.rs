//! Cached binary file reads: small headers followed by configurable payloads.
mod support;
use fast_io::{BufferedInput, Input, Io, ReadError, Source};
use std::{
    error::Error,
    fs::File,
    hint::black_box,
    io::{self, BufReader, Read, Seek, Write},
    time::{Duration, Instant},
};
use support::ScratchFile;

// Deliberately omit read_into to measure the source-buffer copying fallback.
struct Fallback<S>(S);
impl<S: Source> Source for Fallback<S> {
    type Error = S::Error;
    fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        self.0.fill_buf()
    }
    fn consume(&mut self, count: usize) {
        self.0.consume(count);
    }
}

fn standard(mut reader: impl Read, output: &mut [u8], width: usize) -> io::Result<Duration> {
    let start = Instant::now();
    for record in black_box(output).chunks_exact_mut(width) {
        let (header, payload) = record.split_at_mut(8);
        reader.read_exact(header)?;
        reader.read_exact(payload)?;
    }
    Ok(start.elapsed())
}

fn source(
    mut reader: impl Source<Error = io::Error>,
    output: &mut [u8],
    width: usize,
) -> Result<Duration, ReadError<io::Error>> {
    let start = Instant::now();
    for record in black_box(output).chunks_exact_mut(width) {
        let (header, payload) = record.split_at_mut(8);
        fast_io::read_exact(&mut reader, header)?;
        fast_io::read_exact(&mut reader, payload)?;
    }
    Ok(start.elapsed())
}

fn measure(
    mode: usize,
    file: &mut File,
    output: &mut [u8],
    width: usize,
) -> Result<Duration, Box<dyn Error>> {
    file.rewind()?;
    // Allocation, buffer initialization and output poisoning are outside timing.
    output.fill(0xcc);
    Ok(match mode {
        0 => standard(BufReader::with_capacity(8192, file), output, width)?,
        1 => source(Input::with_capacity(8192, file), output, width)?,
        2 => source(BufferedInput::<_, 8192>::new(Io::new(file)), output, width)?,
        3 => source(Fallback(Input::with_capacity(8192, file)), output, width)?,
        _ => unreachable!(),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let count = std::env::args()
        .nth(1)
        .map(|n| n.parse().expect("record count"))
        .unwrap_or(1000usize);
    let payload = std::env::args()
        .nth(2)
        .map(|n| n.parse().expect("payload byte count"))
        .unwrap_or(16_384usize);
    assert!(count > 0 && payload > 0);
    let width = payload.checked_add(8).expect("record too large");
    let mut expected = vec![0; count.checked_mul(width).expect("input too large")];
    for (index, record) in expected.chunks_exact_mut(width).enumerate() {
        record[..8].copy_from_slice(&(index as u64).to_le_bytes());
        for (offset, byte) in record[8..].iter_mut().enumerate() {
            *byte = index.wrapping_mul(31).wrapping_add(offset) as u8;
        }
    }
    let mut file = ScratchFile::new("read")?;
    file.file().write_all(&expected)?;
    let mut output = vec![0; expected.len()];
    let labels = [
        "std BufReader",
        "fast_io Input (direct)",
        "fast_io BufferedInput (inline)",
        "fast_io Source fallback",
    ];
    for (mode, label) in labels.iter().enumerate() {
        measure(mode, file.file(), &mut output, width)?;
        assert_eq!(output, expected, "{label} warmup");
    }
    let mut times = [[Duration::ZERO; 4]; 7];
    for (round, samples) in times.iter_mut().enumerate() {
        for step in 0..labels.len() {
            let mode = (round + step) % labels.len();
            samples[mode] = measure(mode, file.file(), &mut output, width)?;
            assert_eq!(output, expected, "{} sample", labels[mode]);
        }
    }
    println!(
        "{count} records, {} verified bytes, 8-byte headers + {payload}-byte payloads",
        expected.len()
    );
    println!("8192-byte input buffers; cached file; median of 7 rotated samples");
    for (mode, label) in labels.iter().enumerate() {
        let mut samples = times.map(|round| round[mode]);
        samples.sort_unstable();
        println!(
            "{label:31} {:9.2} ns/record, {:8.2} MB/s",
            samples[3].as_nanos() as f64 / count as f64,
            expected.len() as f64 / samples[3].as_secs_f64() / 1_000_000.0
        );
    }
    Ok(())
}
