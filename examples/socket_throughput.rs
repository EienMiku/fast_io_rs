//! Loopback TCP only, including receipt of all bytes by a reader thread.
//! cargo run --release --example socket_throughput -- 10000 32
//! The second argument is the payload size in bytes; try 16384 as well.
use std::{
    io::{self, BufWriter, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum Mode {
    Standard,
    Buffered,
    StandardUnbuffered,
    Scatter,
}
impl Mode {
    const ALL: [Self; 4] = [
        Self::Standard,
        Self::Buffered,
        Self::StandardUnbuffered,
        Self::Scatter,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Standard => "std BufWriter + writeln!",
            Self::Buffered => "fast_io Buffered + println!",
            Self::StandardUnbuffered => "std unbuffered writeln!",
            Self::Scatter => "fast_io Io scatter + println!",
        }
    }

    fn send(self, stream: &TcpStream, count: u64, payload: &str) -> io::Result<()> {
        match self {
            Self::Standard => {
                let mut output = BufWriter::with_capacity(8192, stream);
                standard(&mut output, count, payload)?;
                output.flush()
            }
            Self::Buffered => {
                use fast_io::Sink;
                let mut output = fast_io::Buffered::<_, 8192>::new(fast_io::Io::new(stream));
                fast(&mut output, count, payload)?;
                output.flush()
            }
            Self::Scatter => fast(&mut fast_io::Io::new(stream), count, payload),
            Self::StandardUnbuffered => standard(&mut &*stream, count, payload),
        }
    }
}

fn standard(output: &mut impl Write, count: u64, payload: &str) -> io::Result<()> {
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

fn measure(mode: Mode, count: u64, payload: &str, expected: &[u8]) -> io::Result<Duration> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let expected_len = expected.len();
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
        let (mut input, _) = listener.accept()?;
        input.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut bytes = Vec::with_capacity(expected_len);
        ready_tx.send(()).map_err(io::Error::other)?;
        input.read_to_end(&mut bytes)?;
        Ok(bytes)
    });
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(10))?;
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    ready_rx.recv().map_err(io::Error::other)?;

    let start = Instant::now();
    let sent = mode.send(&stream, count, payload);
    let shutdown = stream.shutdown(Shutdown::Write);
    let received = reader.join().expect("loopback reader panicked");
    let elapsed = start.elapsed();
    sent?;
    shutdown?;
    let actual = received?;
    assert_eq!(actual.len(), expected.len(), "received byte count");
    assert!(
        actual == expected,
        "received bytes differ from standard output"
    );
    Ok(elapsed)
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let count = args
        .next()
        .map(|s| s.parse().expect("record count"))
        .unwrap_or(10_000u64);
    let payload_len = args
        .next()
        .map(|s| s.parse().expect("payload length"))
        .unwrap_or(32usize);
    assert!(count > 0);
    let payload = "x".repeat(payload_len);
    let mut expected = Vec::new();
    standard(&mut expected, count, &payload)?;
    let warmup_count = count.min(1000);
    let mut warmup = Vec::new();
    standard(&mut warmup, warmup_count, &payload)?;
    for mode in Mode::ALL {
        measure(mode, warmup_count, &payload, &warmup)?;
    }

    let mut times: [Vec<Duration>; 4] = std::array::from_fn(|_| Vec::new());
    for round in 0..5 {
        for offset in 0..4 {
            let index = (round + offset) % 4;
            times[index].push(measure(Mode::ALL[index], count, &payload, &expected)?);
        }
    }
    println!("loopback TCP, TCP_NODELAY, 8192-byte persistent buffers, {payload_len}-byte payload");
    println!(
        "{count} records / {} matching bytes per sample; median of 5",
        expected.len()
    );
    println!("timed: formatting, sending, shutdown-write, and receiver completion");
    for (mode, mut samples) in Mode::ALL.into_iter().zip(times) {
        samples.sort_unstable();
        let elapsed = samples[samples.len() / 2];
        let ns = elapsed.as_nanos() as f64 / count as f64;
        let throughput = expected.len() as f64 / elapsed.as_secs_f64() / 1_000_000.0;
        println!(
            "{:32} {ns:10.2} ns/record  {throughput:9.2} MB/s",
            mode.label()
        );
    }
    Ok(())
}
