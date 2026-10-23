//! Run with: cargo run --release --example scan_throughput -- 200000
use fast_io::{Input, Scanner};
use std::{hint::black_box, time::Instant};

fn bench(label: &str, count: usize, expected: u64, mut run: impl FnMut() -> u64) {
    let mut times = [0u128; 5];
    for elapsed in &mut times {
        let start = Instant::now();
        let checksum = black_box(run());
        *elapsed = start.elapsed().as_nanos();
        assert_eq!(checksum, expected);
    }
    times.sort_unstable();
    println!("{label:28} {:9.2} ns/token", times[2] as f64 / count as f64);
}
fn main() {
    let count = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("token count"))
        .unwrap_or(200_000usize);
    assert!(count > 0);
    let mut data = Vec::new();
    let mut expected = 0u64;
    for i in 0..count {
        let value = (i as u64).wrapping_mul(6364136223846793005);
        fast_io::println!(&mut data; value).unwrap();
        expected = expected.wrapping_add(value);
    }
    let text = std::str::from_utf8(&data).unwrap();
    println!(
        "{count} unsigned decimal tokens, {} input bytes",
        data.len()
    );
    bench("std split + parse", count, expected, || {
        black_box(text)
            .split_ascii_whitespace()
            .fold(0u64, |n, s| n.wrapping_add(s.parse::<u64>().unwrap()))
    });
    bench("Scanner<&[u8]>", count, expected, || {
        let mut scan = Scanner::new(black_box(data.as_slice()));
        let mut sum = 0u64;
        while let Some(n) = scan.scan::<u64>().unwrap() {
            sum = sum.wrapping_add(n);
        }
        sum
    });
    bench("Scanner<Input<&[u8]>>", count, expected, || {
        let mut scan = Scanner::new(Input::new(black_box(data.as_slice())));
        let mut sum = 0u64;
        while let Some(n) = scan.scan::<u64>().unwrap() {
            sum = sum.wrapping_add(n);
        }
        sum
    });
}
