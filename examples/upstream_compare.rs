//! Paired with tools/upstream_compare.cpp; run through tools/compare_upstream.py.
use std::{
    hint::black_box,
    io::{self, Write},
    time::Instant,
};

const RECORDS: usize = 1024;
const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

struct Record {
    id: u64,
    delta: i64,
    name: String,
    active: bool,
}

fn records(payload: usize) -> Vec<Record> {
    (0..RECORDS)
        .map(|index| {
            let id = match index {
                0 => 0,
                1 => u64::MAX,
                2 => 1 << 63,
                _ => (index as u64).wrapping_mul(MULTIPLIER),
            };
            let length = if payload == 0 {
                [1, 12, 96][index % 3]
            } else {
                payload
            };
            Record {
                id,
                delta: id as i64,
                name: char::from(b'a' + (index % 26) as u8)
                    .to_string()
                    .repeat(length),
                active: index % 3 != 0,
            }
        })
        .collect()
}

fn encode<S: fast_io::Sink>(output: &mut S, record: &Record) -> Result<(), S::Error> {
    fast_io::println!(output;
        "id=", record.id, " delta=", record.delta,
        " name=", record.name.as_str(), " active=", record.active)
}

fn verify(records: &[Record], path: &str, capacity: usize) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    let mut storage = vec![0; capacity];
    let mut grow = Vec::with_capacity(capacity);
    for record in records {
        let mut slice = fast_io::SliceSink::new(&mut storage);
        encode(&mut slice, record).unwrap();
        grow.clear();
        encode(&mut grow, record).unwrap();
        let expected = format!(
            "id={} delta={} name={} active={}\n",
            record.id, record.delta, record.name, record.active
        );
        assert_eq!(slice.as_bytes(), expected.as_bytes());
        assert_eq!(grow, expected.as_bytes());
        file.write_all(&grow)?;
    }
    Ok(())
}

fn memory(mode: &str, records: &[Record], count: usize, capacity: usize) -> (u128, u64) {
    let mut storage = vec![0; capacity];
    let mut slice = fast_io::SliceSink::new(&mut storage);
    let mut grow = Vec::with_capacity(capacity);
    let mut total = 0u64;
    let start = Instant::now();
    if mode == "slice" {
        for index in 0..count {
            slice.clear();
            encode(&mut slice, black_box(&records[index % RECORDS])).unwrap();
            total += black_box(slice.as_bytes()).len() as u64;
        }
    } else {
        for index in 0..count {
            grow.clear();
            encode(&mut grow, black_box(&records[index % RECORDS])).unwrap();
            total += black_box(grow.as_slice()).len() as u64;
        }
    }
    (start.elapsed().as_nanos(), total)
}

fn parse(data: &[u8]) -> (usize, u64) {
    let mut scanner = fast_io::Scanner::new(black_box(data));
    let mut count = 0;
    let mut sum = 0u64;
    while let Some(value) = scanner.scan::<u64>().unwrap() {
        count += 1;
        sum = sum.wrapping_add(value);
    }
    black_box((count, sum))
}

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 4, "MODE COUNT PAYLOAD [OUTPUT]");
    let mode = args[1].as_str();
    let count: usize = args[2].parse().expect("positive count");
    let payload: usize = args[3].parse().expect("payload length, 0 for mixed");
    assert!(count > 0);
    let records = records(payload);
    let capacity = payload.max(96).checked_add(128).unwrap();
    if mode == "verify" {
        return verify(&records, &args[4], capacity);
    }
    let (elapsed, bytes, checksum) = if mode == "scan" {
        let mut data = Vec::new();
        let mut expected = 0u64;
        for index in 0..count {
            let value = records[index % RECORDS].id;
            writeln!(data, "{value}")?;
            expected = expected.wrapping_add(value);
        }
        assert_eq!(parse(&data), (count, expected));
        let start = Instant::now();
        let actual = parse(&data);
        let elapsed = start.elapsed().as_nanos();
        assert_eq!(actual, (count, expected));
        (elapsed, data.len() as u64, actual.1)
    } else {
        assert!(mode == "slice" || mode == "grow", "unknown mode");
        memory(mode, &records, 10_000, capacity);
        let (elapsed, bytes) = memory(mode, &records, count, capacity);
        (elapsed, bytes, 0)
    };
    println!(
        "{{\"elapsed_ns\":{elapsed},\"units\":{count},\"bytes\":{bytes},\"checksum\":{checksum}}}"
    );
    Ok(())
}
