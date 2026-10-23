//! Run with: cargo run --release --example radix_throughput -- MODE BASE COUNT
//! MODE is std, write, or print; BASE is 2, 8, or 16. Each run verifies all
//! inputs, warms up, then emits one JSON sample. Compare saved before/after
//! executables in alternating order; process startup is outside the timer.
//! `verify BASE PATH` writes all 1024 checked outputs, each followed by LF.
use std::{hint::black_box, io::Write, time::Instant};

const VALUES: usize = 1024;

fn values() -> Vec<u128> {
    (0..VALUES)
        .map(|index| match index {
            0 => 0,
            1 => 1,
            2 => u8::MAX.into(),
            3 => u16::MAX.into(),
            4 => u32::MAX.into(),
            5 => u64::MAX.into(),
            6 => u128::MAX,
            _ => (index as u128).wrapping_mul(0xda94_2042_e4dd_58b5_9e37_79b9_7f4a_7c15),
        })
        .collect()
}

fn direct(storage: &mut [u8; 130], value: u128, base: u8) -> usize {
    let mut sink = fast_io::SliceSink::new(storage);
    fast_io::write(&mut sink, &fast_io::radix(value, base)).unwrap();
    sink.len()
}

fn reserved(storage: &mut [u8; 130], value: u128, base: u8) -> usize {
    let mut sink = fast_io::SliceSink::new(storage);
    fast_io::print!(&mut sink; fast_io::radix(value, base)).unwrap();
    sink.len()
}

fn verify(values: &[u128], base: u8) {
    let mut storage = [0; 130];
    for &value in values {
        let expected = match base {
            2 => format!("{value:b}"),
            8 => format!("{value:o}"),
            16 => format!("{value:x}"),
            _ => unreachable!(),
        };
        let len = direct(&mut storage, value, base);
        assert_eq!(&storage[..len], expected.as_bytes());
        let len = reserved(&mut storage, value, base);
        assert_eq!(&storage[..len], expected.as_bytes());
    }
}

fn measure(
    values: &[u128],
    count: usize,
    mut encode: impl FnMut(&mut [u8; 130], u128) -> usize,
) -> (u128, u64) {
    let mut storage = [0; 130];
    let mut total = 0;
    let start = Instant::now();
    for index in 0..count {
        let used = encode(&mut storage, black_box(values[index % VALUES]));
        total += black_box(&storage[..used]).len() as u64;
    }
    (start.elapsed().as_nanos(), total)
}

fn run(values: &[u128], count: usize, mode: &str, base: u8) -> (u128, u64) {
    macro_rules! standard {
        ($format:literal) => {
            measure(values, count, |storage, value| {
                let mut output = storage.as_mut_slice();
                write!(output, $format, value).unwrap();
                130 - output.len()
            })
        };
    }
    match (mode, base) {
        ("std", 2) => standard!("{:b}"),
        ("std", 8) => standard!("{:o}"),
        ("std", 16) => standard!("{:x}"),
        ("write", _) => measure(values, count, |output, value| direct(output, value, base)),
        ("print", _) => measure(values, count, |output, value| reserved(output, value, base)),
        _ => panic!("MODE must be std, write, or print"),
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "MODE BASE COUNT or verify BASE PATH");
    let mode = &args[1];
    let base: u8 = args[2].parse().unwrap();
    assert!(matches!(base, 2 | 8 | 16), "BASE must be 2, 8, or 16");
    let values = values();
    verify(&values, base);
    if mode == "verify" {
        let mut file = std::fs::File::create(&args[3]).unwrap();
        let mut storage = [0; 130];
        for value in values {
            let len = reserved(&mut storage, value, base);
            file.write_all(&storage[..len]).unwrap();
            file.write_all(b"\n").unwrap();
        }
        return;
    }
    let count: usize = args[3].parse().unwrap();
    assert!(count > 0);
    run(&values, 10_000, mode, base);
    let (elapsed, bytes) = run(&values, count, mode, base);
    println!("{{\"elapsed_ns\":{elapsed},\"units\":{count},\"bytes\":{bytes}}}");
}
