//! Cross-platform anonymous pipes via a child copy of this benchmark.
#[path = "support/output.rs"]
mod output;
use output::{Mode, standard};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Receiver(Child);
impl Drop for Receiver {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn receive(count: u64, payload: &str) -> io::Result<()> {
    let mut expected = Vec::new();
    standard(&mut expected, count, payload)?;
    let mut bytes = Vec::with_capacity(expected.len());
    let mut stdout = io::stdout().lock();
    stdout.write_all(b"ready\n")?;
    stdout.flush()?;
    io::stdin().lock().read_to_end(&mut bytes)?;
    // Acknowledge receipt before comparing; validation is outside the timer.
    stdout.write_all(b"received\n")?;
    stdout.flush()?;
    if bytes != expected {
        return Err(io::Error::other("pipe bytes differ from expected output"));
    }
    Ok(())
}

fn message(reader: &mut impl BufRead, expected: &str) -> io::Result<()> {
    let mut text = String::new();
    reader.read_line(&mut text)?;
    if text != expected {
        return Err(io::Error::other("pipe receiver protocol failed"));
    }
    Ok(())
}

fn measure(mode: Mode, count: u64, payload: &str) -> io::Result<Duration> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--receive", &count.to_string(), &payload.len().to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut receiver = Receiver(command.spawn()?);
    let mut writer = receiver.0.stdin.take().unwrap();
    let mut reader = BufReader::new(receiver.0.stdout.take().unwrap());
    message(&mut reader, "ready\n")?;

    let start = Instant::now();
    let sent = mode.send(&mut writer, count, payload);
    drop(writer); // EOF lets the receiver finish its read_to_end.
    let received = message(&mut reader, "received\n");
    let elapsed = start.elapsed();
    sent?;
    received?;
    // Exit success confirms every byte matched, after the timed receipt.
    if !receiver.0.wait()?.success() {
        return Err(io::Error::other("pipe receiver validation failed"));
    }
    Ok(elapsed)
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let child = first.as_deref() == Some("--receive");
    let count = if child { args.next() } else { first }
        .map(|n| n.parse().expect("record count"))
        .unwrap_or(10_000u64);
    let payload_len = args
        .next()
        .map(|n| n.parse().expect("payload byte count"))
        .unwrap_or(32usize);
    assert!(count > 0);
    let payload = "x".repeat(payload_len);
    if child {
        return receive(count, &payload);
    }
    let mut expected = Vec::new();
    standard(&mut expected, count, &payload)?;
    for mode in Mode::ALL {
        measure(mode, count.min(1000), &payload)?;
    }
    let mut times = [[Duration::ZERO; 4]; 5];
    for (round, samples) in times.iter_mut().enumerate() {
        for step in 0..4 {
            let index = (round + step) % 4;
            samples[index] = measure(Mode::ALL[index], count, &payload)?;
        }
    }
    println!(
        "anonymous child-process pipe; {count} records / {} verified bytes; {payload_len}-byte payload",
        expected.len()
    );
    println!("8192-byte output buffers; median of 5 rotated samples; process setup outside timing");
    println!("timed: formatting, sending, closing the pipe, and receipt acknowledgement");
    for (index, mode) in Mode::ALL.into_iter().enumerate() {
        let mut samples = times.map(|round| round[index]);
        samples.sort_unstable();
        println!(
            "{:32} {:10.2} ns/record  {:9.2} MB/s",
            mode.label(),
            samples[2].as_nanos() as f64 / count as f64,
            expected.len() as f64 / samples[2].as_secs_f64() / 1_000_000.0
        );
    }
    Ok(())
}
