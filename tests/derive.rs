#![cfg(feature = "derive")]
use fast_io::{Encode, Presence, Sink};

#[derive(Encode)]
#[encode("User({name}, {id}){{ok}}")]
struct User<'a, T> {
    name: &'a str,
    id: T,
    #[encode(skip)]
    _internal: NotEncode,
}
struct NotEncode;

#[derive(Encode)]
#[encode(transparent)]
struct Text<'a>(&'a str);

#[derive(Encode)]
struct Pair<T>(T, u8);

struct Streaming;
impl Encode for Streaming {
    fn encode<S: Sink + ?Sized>(&self, output: &mut S) -> Result<(), S::Error> {
        output.write_all(b"stream")
    }
}
#[derive(Encode)]
struct Wrapped(Streaming);

#[test]
fn derived_fields_formats_generics_and_conditional_capabilities() {
    let mut storage = [0; 256];
    let mut output = fast_io::SliceSink::new(&mut storage);
    fast_io::print!(&mut output;
        User { name: "name", id: 42u16, _internal: NotEncode }, " ",
        Text("plain"), " ", Pair(7u8, 9), " ", Wrapped(Streaming)
    )
    .unwrap();
    assert_eq!(
        output.as_bytes(),
        b"User(name, 42){ok} plain Pair(7, 9) Wrapped(stream)"
    );
    assert!(const { fast_io::provided(User::<u16>::reserve_size) });
    assert!(!const { fast_io::provided(Wrapped::reserve_size) });
    assert!(const { fast_io::provided(Text::borrowed_bytes) });
    assert_eq!(Text("a").borrowed_bytes().into_value(), b"a");
}
