//! Read whitespace-separated i64 values and print their sum.
use fast_io::{Buffered, Input, Io, Scanner};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = Scanner::new(Input::new(stdin.lock()));
    let mut output = Buffered::<_>::new(Io::new(stdout.lock()));
    let mut sum = 0i128;
    let mut count = 0usize;
    while let Some(value) = input.scan::<i64>()? {
        sum = sum.checked_add(value as i128).ok_or("sum overflow")?;
        count += 1;
    }
    fast_io::println!(&mut output; "count=", count, " sum=", sum)?;
    output.finish()?;
    Ok(())
}
