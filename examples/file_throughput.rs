//! Buffered file output; flushes to the OS but does not request durable storage.
use fast_io::{Buffered, Io};
use std::{
    fs::{File, OpenOptions},
    hint::black_box,
    io::{self, BufWriter, Read, Seek, Write},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Own only a newly created temporary file; never truncate a pre-existing path.
struct ScratchFile {
    file: Option<File>,
    path: PathBuf,
}
impl ScratchFile {
    fn new(label: &str) -> io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fast-io-bench-{}-{stamp}-{label}.tmp",
            std::process::id()
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            file: Some(file),
            path,
        })
    }
    fn file(&mut self) -> &mut File {
        self.file.as_mut().unwrap()
    }
}
impl Drop for ScratchFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

fn measure(file: &mut File, count: u64, fast: bool, name: &str) -> io::Result<Duration> {
    file.rewind()?;
    file.set_len(0)?;
    let start = Instant::now();
    let name = black_box(name);
    if fast {
        let mut output = Buffered::<_, 8192>::new(Io::new(file));
        for i in 0..count {
            let n = black_box(i.wrapping_mul(6364136223846793005));
            fast_io::println!(&mut output;
                "id=", n, " delta=", n as i64, " name=", name, " active=", true
            )?;
        }
        output.finish()?;
    } else {
        let mut output = BufWriter::with_capacity(8192, file);
        for i in 0..count {
            let n = black_box(i.wrapping_mul(6364136223846793005));
            writeln!(output, "id={n} delta={} name={name} active=true", n as i64)?;
        }
        output.flush()?;
    }
    Ok(start.elapsed())
}

fn compare(left: &mut File, right: &mut File) -> io::Result<u64> {
    let bytes = left.metadata()?.len();
    assert_eq!(bytes, right.metadata()?.len());
    left.rewind()?;
    right.rewind()?;
    let mut a = [0; 8192];
    let mut b = [0; 8192];
    loop {
        let n = left.read(&mut a)?;
        if n == 0 {
            return Ok(bytes);
        }
        right.read_exact(&mut b[..n])?;
        assert_eq!(&a[..n], &b[..n]);
    }
}

fn main() -> io::Result<()> {
    let count = std::env::args()
        .nth(1)
        .map(|n| n.parse().expect("record count"))
        .unwrap_or(100_000u64);
    assert!(count > 0);
    let name = std::env::args().nth(2).map_or_else(
        || String::from("example-user"),
        |length| "x".repeat(length.parse().expect("payload byte count")),
    );
    let mut standard = ScratchFile::new("std")?;
    let mut fast = ScratchFile::new("fast")?;
    let warmup = (1_000_000 / name.len().saturating_add(64)).clamp(1, 5000) as u64;
    measure(standard.file(), warmup, false, &name)?;
    measure(fast.file(), warmup, true, &name)?;
    let mut times = [[Duration::ZERO; 2]; 3];
    for (round, samples) in times.iter_mut().enumerate() {
        if round % 2 == 0 {
            samples[0] = measure(standard.file(), count, false, &name)?;
            samples[1] = measure(fast.file(), count, true, &name)?;
        } else {
            samples[1] = measure(fast.file(), count, true, &name)?;
            samples[0] = measure(standard.file(), count, false, &name)?;
        }
    }
    let bytes = compare(standard.file(), fast.file())?;
    println!(
        "{count} records, {bytes} identical output bytes, {}-byte payloads, 8192-byte buffers",
        name.len()
    );
    println!("OS cache included; flush included; sync_all not requested");
    for (index, label) in ["std BufWriter + writeln!", "fast_io Buffered + println!"]
        .into_iter()
        .enumerate()
    {
        let mut samples = times.map(|round| round[index]);
        samples.sort_unstable();
        println!(
            "{label:30} {:9.2} ns/record, {:8.2} MB/s (median of 3)",
            samples[1].as_nanos() as f64 / count as f64,
            bytes as f64 / samples[1].as_secs_f64() / 1_000_000.0
        );
    }
    Ok(())
}
