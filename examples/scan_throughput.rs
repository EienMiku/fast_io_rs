//! Run with: cargo run --release --example scan_throughput -- 200000
use fast_io::{BufferedInput, Input, Scanner};
use std::{hint::black_box, time::Instant};

fn standard(data: &[u8]) -> u64 {
    std::str::from_utf8(data)
        .unwrap()
        .split_ascii_whitespace()
        .fold(0u64, |n, s| n.wrapping_add(s.parse::<u64>().unwrap()))
}
fn scan<S: fast_io::Source, const STREAM: bool>(source: S) -> u64
where
    S::Error: std::fmt::Debug,
{
    let mut scan = Scanner::new(source);
    let mut sum = 0u64;
    while let Some(n) = if STREAM {
        scan.scan::<u64>()
    } else {
        scan.scan_with(<u64 as fast_io::Scan>::scan_token)
    }
    .unwrap()
    {
        sum = sum.wrapping_add(n);
    }
    sum
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
    println!(
        "{count} unsigned decimal tokens, {} input bytes",
        data.len()
    );
    type Parser = fn(&[u8]) -> u64;
    let parsers: [(&str, Parser); 5] = [
        ("std UTF-8 + split + parse", standard),
        ("Scanner<&[u8]>", |bytes| scan::<_, true>(bytes)),
        ("Scanner<Input<&[u8]>>", |bytes| {
            scan::<_, true>(Input::new(bytes))
        }),
        ("8-byte input, streaming", |bytes| {
            scan::<_, true>(BufferedInput::<_, 8>::new(bytes))
        }),
        ("8-byte input, copied token", |bytes| {
            scan::<_, false>(BufferedInput::<_, 8>::new(bytes))
        }),
    ];
    for (_, parser) in parsers {
        assert_eq!(black_box(parser(black_box(&data))), expected);
    }
    let mut times = [[0u128; 5]; 5];
    for (round, samples) in times.iter_mut().enumerate() {
        // Rotate the first parser so no implementation always runs first.
        for step in 0..parsers.len() {
            let index = (round + step) % parsers.len();
            let start = Instant::now();
            let checksum = black_box(parsers[index].1(black_box(&data)));
            samples[index] = start.elapsed().as_nanos();
            assert_eq!(checksum, expected);
        }
    }
    for (index, (label, _)) in parsers.into_iter().enumerate() {
        let mut samples = times.map(|round| round[index]);
        samples.sort_unstable();
        println!(
            "{label:28} {:9.2} ns/token",
            samples[2] as f64 / count as f64
        );
    }
}
