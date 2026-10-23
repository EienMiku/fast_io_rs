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

#[test]
fn wide_layouts_compile_with_default_limits_and_preserve_argument_order() {
    macro_rules! check_wide {
        ($($field:ident),+) => {{
            #[derive(Encode)]
            struct Wide { $($field: u8),+ }
            #[derive(Encode)]
            enum WideEnum {
                Fields { $($field: u8),+ },
                Empty,
            }
            assert!(const { fast_io::provided(Wide::reserve_size) });
            assert!(const { fast_io::provided(WideEnum::reserve_size) });
            let fields = [$(concat!(stringify!($field), ": 1")),+].join(", ");
            let mut bytes = [0; 2048];
            let mut output = fast_io::SliceSink::new(&mut bytes);
            fast_io::print!(&mut output; Wide { $($field: 1),+ }).unwrap();
            assert_eq!(output.as_bytes(), format!("Wide {{ {fields} }}").as_bytes());
            output.clear();
            fast_io::print!(&mut output; WideEnum::Fields { $($field: 1),+ }).unwrap();
            assert_eq!(output.as_bytes(), format!("WideEnum::Fields {{ {fields} }}").as_bytes());
            output.clear();
            fast_io::print!(&mut output; WideEnum::Empty).unwrap();
            assert_eq!(output.as_bytes(), b"WideEnum::Empty");
            output.clear();
            let mut sequence = 0u8;
            fast_io::print!(&mut output; $({
                let _ = stringify!($field);
                let value = sequence;
                sequence += 1;
                value
            }),+).unwrap();
            assert_eq!(sequence, 64);
            let expected: String = (0u8..64).map(|n| n.to_string()).collect();
            assert_eq!(output.as_bytes(), expected.as_bytes());
        }};
    }
    check_wide!(
        f00, f01, f02, f03, f04, f05, f06, f07, f08, f09, f10, f11, f12, f13, f14, f15, f16, f17,
        f18, f19, f20, f21, f22, f23, f24, f25, f26, f27, f28, f29, f30, f31, f32, f33, f34, f35,
        f36, f37, f38, f39, f40, f41, f42, f43, f44, f45, f46, f47, f48, f49, f50, f51, f52, f53,
        f54, f55, f56, f57, f58, f59, f60, f61, f62, f63
    );
}

#[test]
fn explicit_field_formatters_preserve_bounds_reservations_and_batch_order() {
    // Only Debug is needed for D; requiring Encode here would reject DebugOnly.
    #[derive(Debug)]
    struct DebugOnly;
    #[derive(Encode)]
    #[encode("{type:#x}/{value:?}/{type:#X}")]
    struct Mixed<T, D> {
        r#type: T,
        value: D,
    }
    use fast_io as renamed;
    #[derive(renamed::Encode)]
    #[encode(crate = "renamed")]
    #[encode("{0:x}/{0:X}/{0:b}/{0:o}/{0:#x}/{0:#X}/{0:#b}/{0:#o}/{0:}")]
    struct Radices<T>(T);
    #[derive(Encode)]
    enum RadixEvent<T> {
        #[encode("[{number:#x}]")]
        Named { number: T },
        #[encode("<{0:#b}>")]
        Tuple(T),
    }

    assert!(const { fast_io::provided(Radices::<u128>::reserve_size) });
    assert!(const { fast_io::provided2(Radices::<u128>::encode_reserved) });
    assert!(const { fast_io::provided(RadixEvent::<u64>::reserve_size) });
    assert!(!const { fast_io::provided(Mixed::<u64, DebugOnly>::reserve_size) });
    assert!(!const { fast_io::provided2(Mixed::<u64, DebugOnly>::encode_reserved) });

    let mut storage = [0; 1024];
    let mut output = fast_io::SliceSink::new(&mut storage);
    for value in [0, 42, u128::MAX] {
        let expected = format!(
            "{value:x}/{value:X}/{value:b}/{value:o}/{value:#x}/0X{value:X}/{value:#b}/{value:#o}/{value}"
        );
        let formatted = Radices(value);
        // The native decimal field advertises an upper bound rather than the
        // current value's exact length.
        assert!(formatted.reserve_size().into_value() >= expected.len());
        output.clear();
        fast_io::print!(&mut output; formatted).unwrap();
        assert_eq!(output.as_bytes(), expected.as_bytes());
        // The same fields through tiny scratch storage exercise repeated drains
        // and temporary formatters, without retaining references to temporaries.
        output.clear();
        let mut batch = fast_io::Batch::<_, 4, 2>::new(&mut output);
        batch.push(&formatted).unwrap();
        batch.finish().unwrap();
        assert_eq!(output.as_bytes(), expected.as_bytes());
    }
    output.clear();
    fast_io::print!(&mut output;
        RadixEvent::Named { number: 255u64 }, " ", RadixEvent::Tuple(5u64), " ",
        Mixed { r#type: 42u64, value: DebugOnly }
    )
    .unwrap();
    assert_eq!(output.as_bytes(), b"[0xff] <0b101> 0x2a/DebugOnly/0X2A");
}

#[cfg(feature = "float")]
#[test]
fn literal_float_precision_uses_the_existing_reserved_formatters() {
    use fast_io as renamed;
    #[derive(renamed::Encode)]
    #[encode(crate = "renamed")]
    #[encode("{type:.2}/{type:.3e}/{type:.3E}/{type:.0}")]
    struct Number<T> {
        r#type: T,
    }
    #[derive(Encode)]
    enum NumberEvent<T> {
        #[encode("<{0:.1E}>")]
        Value(T),
    }
    assert!(const { fast_io::provided(Number::<f64>::reserve_size) });
    assert!(const { fast_io::provided2(Number::<f64>::encode_reserved) });
    assert!(const { fast_io::provided(NumberEvent::<f32>::reserve_size) });
    let mut storage = [0; 2048];
    let mut output = fast_io::SliceSink::new(&mut storage);
    for value in [-0.0, 9.995, f64::MAX, f64::INFINITY] {
        let number = Number { r#type: value };
        let expected = format!("{value:.2}/{value:.3e}/{value:.3E}/{value:.0}");
        output.clear();
        fast_io::print!(&mut output; number).unwrap();
        assert_eq!(output.as_bytes(), expected.as_bytes());
        output.clear();
        let mut batch = fast_io::Batch::<_, 8, 2>::new(&mut output);
        batch.push(&number).unwrap();
        batch.finish().unwrap();
        assert_eq!(output.as_bytes(), expected.as_bytes());
    }
    output.clear();
    fast_io::print!(&mut output; NumberEvent::Value(12.5f32)).unwrap();
    assert_eq!(output.as_bytes(), b"<1.2E1>");
}
