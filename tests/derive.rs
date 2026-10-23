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
// Deliberately awkward names exercise the generated code's hygiene.
#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    clippy::builtin_type_shadow
)]
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

    #[derive(Encode)]
    #[encode("{type}/{bytes}")]
    struct Raw<const output: usize, const batch: usize> {
        r#type: u8,
        bytes: [u8; output],
        #[encode(skip)]
        _unused: [u8; batch],
    }
    #[derive(Encode)]
    enum Collision<const output: usize, const batch: usize, const __fast_io_field_0: usize> {
        Value([u8; output], [u8; batch], [u8; __fast_io_field_0]),
    }
    output.clear();
    fast_io::print!(&mut output;
        Raw { r#type: 3, bytes: *b"ab", _unused: [] }, " ",
        Collision::Value(*b"a", *b"b", *b"c")
    )
    .unwrap();
    assert_eq!(output.as_bytes(), b"3/ab Collision::Value(a, b, c)");
    #[derive(Encode)]
    #[encode("{0}/{1}/{2}")]
    struct TypeNames<usize, u8, Sized>(usize, u8, Sized);
    output.clear();
    fast_io::print!(&mut output; TypeNames(1u8, 2u8, "ok")).unwrap();
    assert_eq!(output.as_bytes(), b"1/2/ok");
}

#[derive(Encode)]
enum Event<'a, T> {
    Ready,
    Value {
        message: &'a str,
        count: T,
    },
    #[encode("<{0}>")]
    Raw(T),
    #[encode(transparent)]
    Text(&'a str),
    Hidden(#[encode(skip)] NotEncode),
}

#[test]
fn derived_enum_variants_preserve_static_field_dispatch() {
    assert!(const { fast_io::provided(Event::<u8>::reserve_size) });
    assert!(const { fast_io::provided2(Event::<u8>::encode_reserved) });
    assert!(!const { fast_io::provided(Event::<Streaming>::reserve_size) });
    assert!(!const { fast_io::provided2(Event::<Streaming>::encode_reserved) });
    let mut storage = [0; 256];
    let mut output = fast_io::SliceSink::new(&mut storage);
    fast_io::print!(&mut output;
        Event::<u8>::Ready, " ",
        Event::Value { message: "x", count: 1u8 }, " ",
        Event::Raw(4u8), " ", Event::<u8>::Text("plain"), " ",
        Event::<u8>::Hidden(NotEncode)
    )
    .unwrap();
    assert_eq!(
        output.as_bytes(),
        b"Event::Ready Event::Value { message: x, count: 1 } <4> plain Event::Hidden()"
    );
    output.clear();
    fast_io::print!(&mut output; Event::Raw(Streaming)).unwrap();
    assert_eq!(output.as_bytes(), b"<stream>");
}
