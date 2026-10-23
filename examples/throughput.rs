//! Run with: cargo run --release --example throughput -- 500000
//! In-memory formatting only: these numbers do not measure OS or disk throughput.
use std::{
    hint::black_box,
    io::Write,
    time::{Duration, Instant},
};

fn measure(iterations: u64, mut encode: impl FnMut(&mut Vec<u8>, u64, i64, &str)) -> Duration {
    let mut output = Vec::with_capacity(256);
    let name = black_box("example-user");
    let start = Instant::now();
    for i in 0..iterations {
        output.clear();
        let n = black_box(i.wrapping_mul(6364136223846793005));
        encode(&mut output, n, n as i64, name);
        black_box(output.as_slice());
    }
    start.elapsed()
}

fn report(label: &str, values: &mut [Duration], iterations: u64) {
    values.sort_unstable();
    let median = values[values.len() / 2].as_nanos() as f64 / iterations as f64;
    println!(
        "{label:24} {median:9.2} ns/record (median of {})",
        values.len()
    );
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("iteration count"))
        .unwrap_or(500_000u64);
    assert!(iterations > 0);
    let standard = |out: &mut Vec<u8>, n: u64, signed: i64, name: &str| {
        writeln!(out, "id={n} delta={signed} name={name} active=true").unwrap();
    };
    let fast = |out: &mut Vec<u8>, n, signed, name: &str| {
        fast_io::println!(out; "id=", n, " delta=", signed, " name=", name, " active=", true)
            .unwrap();
    };
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    standard(&mut expected, 987654321, -1234567, "example-user");
    fast(&mut actual, 987654321u64, -1234567i64, "example-user");
    assert_eq!(actual, expected);

    // Warm up both code paths before alternating measurements.
    measure(10_000, standard);
    measure(10_000, fast);
    let mut std_times = Vec::new();
    let mut fast_times = Vec::new();
    for round in 0..5 {
        if round % 2 == 0 {
            std_times.push(measure(iterations, standard));
            fast_times.push(measure(iterations, fast));
        } else {
            fast_times.push(measure(iterations, fast));
            std_times.push(measure(iterations, standard));
        }
    }
    println!("mixed record -> reused Vec<u8>; {iterations} records/sample");
    report("std::writeln!", &mut std_times, iterations);
    report("fast_io::println!", &mut fast_times, iterations);
}
