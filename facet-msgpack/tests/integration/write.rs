use facet::Facet;
use facet_msgpack::to_vec;

#[test]
fn test_integers() {
    facet_testhelpers::setup(); // Revert to direct call

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct IntegerTest {
        pos_fixint: u8, // 0-127
        uint8: u8,      // 128-255
        uint16: u16,    // 256-65535
        uint32: u32,    // up to 4294967295
        uint64: u64,    // larger values
        neg_fixint: i8, // -32 to -1
        int8: i8,       // -128 to -33
        int16: i16,     // -32768 to 32767
        int32: i32,     // -2147483648 to 2147483647
        int64: i64,     // -9223372036854775808 to 9223372036854775807
    }

    let test = IntegerTest {
        pos_fixint: 127,
        uint8: 255,
        uint16: 65535,
        uint32: 4294967295,
        uint64: 18446744073709551615,
        neg_fixint: -1,
        int8: -128,
        int16: -32768,
        int32: -2147483648,
        int64: -9223372036854775808,
    };

    let msgpack = to_vec(&test);

    // Expected format:
    // 8a                -- map with 10 elements
    // aa706f735f666978696e74  -- "pos_fixint"
    // 7f                -- 127 (positive fixint)
    // a675696e7438     -- "uint8"
    // cc ff            -- 255 (uint8)
    // a775696e743136   -- "uint16"
    // cd ff ff         -- 65535 (uint16)
    // a775696e743332   -- "uint32"
    // ce ff ff ff ff   -- 4294967295 (uint32)
    // a775696e743634   -- "uint64"
    // cf ff ff ff ff ff ff ff ff  -- 18446744073709551615 (uint64)
    // aa6e65675f666978696e74  -- "neg_fixint"
    // ff               -- -1 (negative fixint)
    // a4696e7438       -- "int8"
    // d0 80            -- -128 (int8)
    // a5696e743136     -- "int16"
    // d1 80 00         -- -32768 (int16)
    // a5696e743332     -- "int32"
    // d2 80 00 00 00   -- -2147483648 (int32)
    // a5696e743634     -- "int64"
    // d3 80 00 00 00 00 00 00 00  -- -9223372036854775808 (int64)

    let expected = [
        0x8a, // map with 10 elements
        // pos_fixint
        0xaa, 0x70, 0x6f, 0x73, 0x5f, 0x66, 0x69, 0x78, 0x69, 0x6e, 0x74, 0x7f, // 127
        // uint8
        0xa5, 0x75, 0x69, 0x6e, 0x74, 0x38, 0xcc, 0xff, // 255
        // uint16
        0xa6, 0x75, 0x69, 0x6e, 0x74, 0x31, 0x36, 0xcd, 0xff, 0xff, // 65535
        // uint32
        0xa6, 0x75, 0x69, 0x6e, 0x74, 0x33, 0x32, 0xce, 0xff, 0xff, 0xff, 0xff, // 4294967295
        // uint64
        0xa6, 0x75, 0x69, 0x6e, 0x74, 0x36, 0x34, 0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, // 18446744073709551615
        // neg_fixint
        0xaa, 0x6e, 0x65, 0x67, 0x5f, 0x66, 0x69, 0x78, 0x69, 0x6e, 0x74, 0xff, // -1
        // int8
        0xa4, 0x69, 0x6e, 0x74, 0x38, 0xd0, 0x80, // -128
        // int16
        0xa5, 0x69, 0x6e, 0x74, 0x31, 0x36, 0xd1, 0x80, 0x00, // -32768
        // int32
        0xa5, 0x69, 0x6e, 0x74, 0x33, 0x32, 0xd2, 0x80, 0x00, 0x00, 0x00, // -2147483648
        // int64
        0xa5, 0x69, 0x6e, 0x74, 0x36, 0x34, 0xd3, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // -9223372036854775808
    ];

    assert_eq!(
        msgpack, expected,
        "\nGot:     {:02x?}\nExpected: {:02x?}",
        msgpack, expected
    );
}

#[test]
fn test_struct() {
    facet_testhelpers::setup(); // Revert to direct call

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }

    let test_struct = TestStruct {
        name: "Alice".to_string(),
        age: 30,
    };

    let msgpack = to_vec(&test_struct);

    // Expected MessagePack format:
    // 82                -- map with 2 elements
    // a4 6e616d65      -- "name" (length 4)
    // a5 416c696365    -- "Alice" (length 5)
    // a3 616765        -- "age" (length 3)
    // 1e               -- 30 as positive fixint
    let expected = [
        0x82, 0xa4, 0x6e, 0x61, 0x6d, 0x65, 0xa5, 0x41, 0x6c, 0x69, 0x63, 0x65, 0xa3, 0x61, 0x67,
        0x65, 0x1e,
    ];

    assert_eq!(msgpack, expected);
}
