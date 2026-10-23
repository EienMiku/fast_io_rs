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

#[test]
fn byte_padding_uses_reserved_and_streaming_paths() {
    assert_eq!(fast_io::format_bytes!("n=", 7), b"n=7");
    assert_eq!(fast_io::format_text!("n=", 7).unwrap(), "n=7");
    assert!(fast_io::format_text!([0xffu8]).is_err());
    let mut output = Vec::new();
    fast_io::print!(&mut output;
        fast_io::pad(42u8, 5).fill(b'0'), "|",
        fast_io::pad("é", 6).center().fill(b'.'), "|",
        fast_io::pad(fast_io::display(7), 3), "|",
        fast_io::pad("long", 2).left()
    )
    .unwrap();
    assert_eq!(output, "00042|..é..|  7|long".as_bytes());
    let mut padded = Vec::new();
    fast_io::print!(&mut padded; fast_io::pad(1u8, 300)).unwrap();
    assert_eq!(padded.len(), 300);
    assert_eq!(padded[299], b'1');
}

#[test]
fn utf16_conversion_preserves_pairs_and_handles_unpaired_surrogates() {
    let text = "a中文🦀";
    let units: Vec<u16> = text.encode_utf16().collect();
    assert_eq!(
        fast_io::format_bytes!(fast_io::utf16(&units).unwrap()),
        text.as_bytes()
    );
    assert_eq!(
        fast_io::format_bytes!(fast_io::utf16_le(text)),
        units
            .iter()
            .flat_map(|u| u.to_le_bytes())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fast_io::format_bytes!(fast_io::utf16_be(text)),
        units
            .iter()
            .flat_map(|u| u.to_be_bytes())
            .collect::<Vec<_>>()
    );
    let bad = [b'a' as u16, 0xd800, b'b' as u16, 0xdc00];
    assert_eq!(fast_io::utf16(&bad).unwrap_err().index, 1);
    assert_eq!(
        fast_io::format_bytes!(fast_io::utf16_lossy(&bad)),
        "a�b�".as_bytes()
    );
    let long = "中文🦀".repeat(100);
    let mut streamed = Vec::new();
    fast_io::Encode::encode(&fast_io::utf16_le(&long), &mut streamed).unwrap();
    assert_eq!(
        streamed,
        long.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
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
