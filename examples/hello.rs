use fast_io::{Io, Sink};

fn main() -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = Io::new(stdout.lock());
    fast_io::println!(&mut out; "Hello from Rust: ", 42, " / ", true, " / ", '🦀')?;
    out.flush()?;
    fast_io::println!("Default stdout also returns an I/O result.")
}
