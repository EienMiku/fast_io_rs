//! Run with: cargo run --release --features derive --example derive_throughput -- 500000
//! Prebuilt records isolate formatting from constructing/allocating the input.
use std::{
    hint::black_box,
    io::Write,
    time::{Duration, Instant},
};

#[derive(fast_io::Encode)]
#[encode("id={id} delta={delta} name={name} active={active}")]
struct Row {
    id: u64,
    delta: i64,
    name: &'static str,
    active: bool,
}

#[derive(fast_io::Encode)]
enum Record {
    #[encode(transparent)]
    Full(Row),
    #[encode("id={id} name={name}")]
    Compact { id: u64, name: &'static str },
}

fn standard_row(out: &mut Vec<u8>, row: &Row) {
    writeln!(
        out,
        "id={} delta={} name={} active={}",
        row.id, row.delta, row.name, row.active
    )
    .unwrap();
}

fn direct_row(out: &mut Vec<u8>, row: &Row) {
    fast_io::println!(out;
        "id=", row.id, " delta=", row.delta, " name=", row.name, " active=", row.active
    )
    .unwrap();
}

fn standard_record(out: &mut Vec<u8>, record: &Record) {
    match record {
        Record::Full(row) => standard_row(out, row),
        Record::Compact { id, name } => writeln!(out, "id={id} name={name}").unwrap(),
    }
}

fn direct_record(out: &mut Vec<u8>, record: &Record) {
    match record {
        Record::Full(row) => direct_row(out, row),
        Record::Compact { id, name } => fast_io::println!(out; "id=", id, " name=", name).unwrap(),
    }
}

fn derived<T: fast_io::Encode>(out: &mut Vec<u8>, value: &T) {
    fast_io::println!(out; value).unwrap();
}

fn measure<T>(
    values: &[T],
    iterations: usize,
    mut encode: impl FnMut(&mut Vec<u8>, &T),
) -> Duration {
    let mut output = Vec::with_capacity(256);
    let start = Instant::now();
    for value in values.iter().cycle().take(iterations) {
        output.clear();
        encode(&mut output, black_box(value));
        black_box(output.as_slice());
    }
    start.elapsed()
}

fn compare<T>(
    label: &str,
    values: &[T],
    iterations: usize,
    standard: impl Fn(&mut Vec<u8>, &T) + Copy,
    direct: impl Fn(&mut Vec<u8>, &T) + Copy,
    derived: impl Fn(&mut Vec<u8>, &T) + Copy,
) {
    // Check every input and warm up each monomorphized path before timing.
    for value in values {
        let mut expected = Vec::new();
        let mut direct_output = Vec::new();
        let mut derived_output = Vec::new();
        standard(&mut expected, value);
        direct(&mut direct_output, value);
        derived(&mut derived_output, value);
        assert_eq!(direct_output, expected);
        assert_eq!(derived_output, expected);
    }
    measure(values, 10_000, standard);
    measure(values, 10_000, direct);
    measure(values, 10_000, derived);
    let mut samples: [Vec<Duration>; 3] = std::array::from_fn(|_| Vec::new());
    for round in 0..9 {
        for offset in 0..3 {
            let index = (round + offset) % 3;
            let elapsed = match index {
                0 => measure(values, iterations, standard),
                1 => measure(values, iterations, direct),
                _ => measure(values, iterations, derived),
            };
            samples[index].push(elapsed);
        }
    }
    println!("{label} -> reused Vec<u8>; {iterations} records/sample");
    for (label, times) in [
        "std::writeln!",
        "direct fast_io arguments",
        "derived Encode",
    ]
    .into_iter()
    .zip(&mut samples)
    {
        times.sort_unstable();
        let median = times[times.len() / 2].as_nanos() as f64 / iterations as f64;
        println!(
            "{label:26} {median:9.2} ns/record (median of {})",
            times.len()
        );
    }
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("iteration count"))
        .unwrap_or(500_000usize);
    assert!(iterations > 0);
    let row = |index: u64| {
        let id = index.wrapping_mul(6364136223846793005);
        Row {
            id,
            delta: id as i64,
            name: ["short", "example-user", "a slightly longer user name"][index as usize % 3],
            active: index % 2 == 0,
        }
    };
    let rows: Vec<_> = (0..1024).map(row).collect();
    let records: Vec<_> = (0..1024)
        .map(|index| {
            if index % 4 == 0 {
                Record::Compact {
                    id: index,
                    name: "compact",
                }
            } else {
                Record::Full(row(index))
            }
        })
        .collect();
    compare(
        "struct",
        &rows,
        iterations,
        standard_row,
        direct_row,
        derived,
    );
    compare(
        "mixed enum",
        &records,
        iterations,
        standard_record,
        direct_record,
        derived,
    );
}
