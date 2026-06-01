// The extended-kind value types and their canonical string forms.
//
// phon has no primitive *wire* tag for date/time, uuid, or qualified name; each
// rides a dedicated tag carrying its canonical string (`r[value.extended-kinds]`).
// A reader without a native type keeps the string. These format/parse functions
// mirror `rust/phon-schema/src/selfdescribing.rs` so the strings are byte-identical
// across implementations.

// MARK: - DateTime

/// A date and/or time. Which fields are meaningful is set by `kind`.
public struct DateTime: Hashable, Sendable {
    public enum Kind: Hashable, Sendable {
        case localDate
        case localTime
        case localDateTime
        case offset(minutes: Int16)
    }

    public var year: Int32
    public var month: UInt8
    public var day: UInt8
    public var hour: UInt8
    public var minute: UInt8
    public var second: UInt8
    public var nanos: UInt32
    public var kind: Kind

    public init(
        year: Int32 = 0, month: UInt8 = 0, day: UInt8 = 0,
        hour: UInt8 = 0, minute: UInt8 = 0, second: UInt8 = 0, nanos: UInt32 = 0,
        kind: Kind
    ) {
        self.year = year
        self.month = month
        self.day = day
        self.hour = hour
        self.minute = minute
        self.second = second
        self.nanos = nanos
        self.kind = kind
    }
}

// MARK: - Decimal padding (Foundation-free)

/// Left-pad the decimal form of a non-negative integer to `width` with zeros.
private func pad(_ value: Int, _ width: Int) -> String {
    let s = String(value)
    return s.count >= width ? s : String(repeating: "0", count: width - s.count) + s
}

/// Format a year to a minimum width of 4 *including* a leading sign — matching
/// Rust's `{:04}` (e.g. `2026`, `-001`, `-2026`).
private func padYear(_ y: Int32) -> String {
    if y < 0 {
        return "-" + pad(-Int(y), 3)
    }
    return pad(Int(y), 4)
}

// MARK: - DateTime canonical string

/// RFC 3339 / ISO 8601: `T` marks a datetime, `:` a time, `-` a date; fractional
/// seconds are `.` plus nine digits when nonzero; the offset is `Z` or `±HH:MM`.
func datetimeString(_ d: DateTime) -> String {
    let date = "\(padYear(d.year))-\(pad(Int(d.month), 2))-\(pad(Int(d.day), 2))"
    var time = "\(pad(Int(d.hour), 2)):\(pad(Int(d.minute), 2)):\(pad(Int(d.second), 2))"
    if d.nanos != 0 {
        time += "." + pad(Int(d.nanos), 9)
    }
    switch d.kind {
    case .localDate:
        return date
    case .localTime:
        return time
    case .localDateTime:
        return "\(date)T\(time)"
    case .offset(let minutes):
        let offset: String
        if minutes == 0 {
            offset = "Z"
        } else {
            let sign = minutes < 0 ? "-" : "+"
            let abs = Swift.abs(Int(minutes))
            offset = "\(sign)\(pad(abs / 60, 2)):\(pad(abs % 60, 2))"
        }
        return "\(date)T\(time)\(offset)"
    }
}

func parseDatetime(_ s: String) throws -> DateTime {
    let bad = DecodeError.malformed("datetime")
    let sub = Substring(s)
    if let (date, rest) = splitOnce(sub, "T") {
        guard let (y, mo, da) = parseDate(date) else { throw bad }
        // The offset starts at a trailing `Z`, `+`, or `-`; the time has none.
        let time: Substring
        let offsetStr: Substring?
        if let i = rest.firstIndex(where: { $0 == "Z" || $0 == "+" || $0 == "-" }) {
            time = rest[..<i]
            offsetStr = rest[i...]
        } else {
            time = rest
            offsetStr = nil
        }
        guard let (h, mi, se, na) = parseTime(time) else { throw bad }
        if let offsetStr {
            guard let off = parseOffset(offsetStr) else { throw bad }
            return DateTime(year: y, month: mo, day: da, hour: h, minute: mi, second: se, nanos: na, kind: .offset(minutes: off))
        }
        return DateTime(year: y, month: mo, day: da, hour: h, minute: mi, second: se, nanos: na, kind: .localDateTime)
    } else if s.contains(":") {
        guard let (h, mi, se, na) = parseTime(sub) else { throw bad }
        return DateTime(hour: h, minute: mi, second: se, nanos: na, kind: .localTime)
    } else if s.contains("-") {
        guard let (y, mo, da) = parseDate(sub) else { throw bad }
        return DateTime(year: y, month: mo, day: da, kind: .localDate)
    } else {
        throw bad
    }
}

// MARK: - DateTime string parsing helpers

private func splitOnce(_ s: Substring, _ sep: Character) -> (Substring, Substring)? {
    guard let i = s.firstIndex(of: sep) else { return nil }
    return (s[..<i], s[s.index(after: i)...])
}

private func rsplitOnce(_ s: Substring, _ sep: Character) -> (Substring, Substring)? {
    guard let i = s.lastIndex(of: sep) else { return nil }
    return (s[..<i], s[s.index(after: i)...])
}

private func parseDate(_ s: Substring) -> (Int32, UInt8, UInt8)? {
    // `[-]YYYY-MM-DD`: split day and month off the right so a negative year's
    // leading `-` stays with the year.
    guard let (rest, dayS) = rsplitOnce(s, "-"),
          let (yearS, monthS) = rsplitOnce(rest, "-"),
          let year = Int32(yearS), let month = UInt8(monthS), let day = UInt8(dayS)
    else { return nil }
    return (year, month, day)
}

private func parseTime(_ s: Substring) -> (UInt8, UInt8, UInt8, UInt32)? {
    let hms: Substring
    let frac: Substring?
    if let (a, f) = splitOnce(s, ".") {
        hms = a
        frac = f
    } else {
        hms = s
        frac = nil
    }
    let parts = hms.split(separator: ":", omittingEmptySubsequences: false)
    guard parts.count == 3,
          let h = UInt8(parts[0]), let mi = UInt8(parts[1]), let se = UInt8(parts[2])
    else { return nil }

    let nanos: UInt32
    if let frac {
        guard (1...9).contains(frac.count), frac.utf8.allSatisfy({ (48...57).contains($0) }) else {
            return nil
        }
        var padded = String(frac)
        while padded.count < 9 { padded.append("0") }
        guard let n = UInt32(padded) else { return nil }
        nanos = n
    } else {
        nanos = 0
    }
    return (h, mi, se, nanos)
}

private func parseOffset(_ s: Substring) -> Int16? {
    if s == "Z" { return 0 }
    let sign: Int
    switch s.first {
    case "+": sign = 1
    case "-": sign = -1
    default: return nil
    }
    guard let (hh, mm) = splitOnce(s.dropFirst(), ":"),
          let h = Int(hh), let m = Int(mm)
    else { return nil }
    return Int16(exactly: sign * (h * 60 + m))
}

// MARK: - UUID

/// `550e8400-e29b-41d4-a716-446655440000` (lowercase, hyphenated).
func uuidString(_ n: UInt128) -> String {
    var hex = String(n, radix: 16)
    if hex.count < 32 { hex = String(repeating: "0", count: 32 - hex.count) + hex }
    let b = Array(hex.utf8)
    func seg(_ a: Int, _ c: Int) -> String { String(decoding: b[a..<c], as: UTF8.self) }
    return "\(seg(0, 8))-\(seg(8, 12))-\(seg(12, 16))-\(seg(16, 20))-\(seg(20, 32))"
}

func parseUuid(_ s: String) throws -> UInt128 {
    let hex = s.filter { $0 != "-" }
    guard hex.count == 32, let n = UInt128(hex, radix: 16) else {
        throw DecodeError.malformed("uuid")
    }
    return n
}

// MARK: - QName

/// James Clark notation: `{namespace}local`, or `local` with no namespace.
func qnameString(_ namespace: String?, _ local: String) -> String {
    switch namespace {
    case .none: return local
    case .some(let ns): return "{\(ns)}\(local)"
    }
}

func parseQName(_ s: String) throws -> (namespace: String?, local: String) {
    if s.hasPrefix("{") {
        let rest = s.dropFirst()
        guard let i = rest.firstIndex(of: "}") else {
            throw DecodeError.malformed("qname")
        }
        return (String(rest[..<i]), String(rest[rest.index(after: i)...]))
    }
    return (nil, s)
}

// MARK: - Value <-> canonical string

/// The canonical string of an extended-kind value (`datetime`/`uuid`/`qname`),
/// or `nil` if `value` is not that kind. Shared by the compact codec so the
/// canonical form lives in one place.
public func extendedToString(_ value: Value, _ primitive: Primitive) -> String? {
    switch primitive {
    case .datetime:
        guard case .datetime(let d) = value else { return nil }
        return datetimeString(d)
    case .uuid:
        guard case .uuid(let n) = value else { return nil }
        return uuidString(n)
    case .qname:
        guard case .qname(let ns, let local) = value else { return nil }
        return qnameString(ns, local)
    default:
        return nil
    }
}

/// Parse the canonical string of an extended-kind primitive into a `Value`.
public func extendedFromString(_ s: String, _ primitive: Primitive) throws -> Value {
    switch primitive {
    case .datetime:
        return .datetime(try parseDatetime(s))
    case .uuid:
        return .uuid(try parseUuid(s))
    case .qname:
        let (ns, local) = try parseQName(s)
        return .qname(namespace: ns, local: local)
    default:
        throw DecodeError.malformed("not an extended-kind primitive")
    }
}
