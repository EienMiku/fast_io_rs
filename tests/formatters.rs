#![cfg(feature = "alloc")]

#[test]
fn radix_and_json_string_escaping() {
    let mut output = Vec::new();
    fast_io::print!(&mut output;
        fast_io::hex(u128::MAX), " ", fast_io::hex(255u32).uppercase().prefixed(),
        " ", fast_io::bin(5u8).prefixed(), " ", fast_io::radix(35u8, 36),
        " ", fast_io::quoted("a\n\"\\\0中文")
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "ffffffffffffffffffffffffffffffff 0XFF 0b101 z \"a\\n\\\"\\\\\\u0000中文\""
    );
}

#[cfg(feature = "float")]
#[test]
fn floats_round_trip_and_preserve_special_values() {
    for value in [
        0.0f64,
        -0.0,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        1.2345,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let mut bytes = Vec::new();
        fast_io::write(&mut bytes, &value).unwrap();
        let parsed: f64 = std::str::from_utf8(&bytes).unwrap().parse().unwrap();
        assert_eq!(parsed.to_bits(), value.to_bits());
    }
    let mut bytes = Vec::new();
    fast_io::write(&mut bytes, &f32::NAN).unwrap();
    assert_eq!(bytes, b"NaN");
}
