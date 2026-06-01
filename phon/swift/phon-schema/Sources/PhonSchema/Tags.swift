// Self-describing wire tags. Each tag-led value begins with one of these bytes,
// then the body the tag describes. Mirrors `rust/phon-schema/src/selfdescribing.rs`.

enum Tag {
    static let unit: UInt8 = 0x00
    static let bool: UInt8 = 0x01
    static let u8: UInt8 = 0x02
    static let u16: UInt8 = 0x03
    static let u32: UInt8 = 0x04
    static let u64: UInt8 = 0x05
    static let u128: UInt8 = 0x06
    static let i8: UInt8 = 0x07
    static let i16: UInt8 = 0x08
    static let i32: UInt8 = 0x09
    static let i64: UInt8 = 0x0A
    static let i128: UInt8 = 0x0B
    static let f32: UInt8 = 0x0C
    static let f64: UInt8 = 0x0D
    static let char: UInt8 = 0x0E
    static let string: UInt8 = 0x0F
    static let bytes: UInt8 = 0x10
    static let list: UInt8 = 0x11
    static let set: UInt8 = 0x12
    static let map: UInt8 = 0x13
    static let array: UInt8 = 0x14
    static let tuple: UInt8 = 0x15
    static let structure: UInt8 = 0x16
    static let enumeration: UInt8 = 0x17
    static let optionNone: UInt8 = 0x18
    static let optionSome: UInt8 = 0x19
    static let tensor: UInt8 = 0x1A
    static let datetime: UInt8 = 0x1B
    static let uuid: UInt8 = 0x1C
    static let qname: UInt8 = 0x1D
}

let maxDepth = 128
