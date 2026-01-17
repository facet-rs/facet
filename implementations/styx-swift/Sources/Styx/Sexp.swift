/// S-expression formatting for compliance testing.
extension Document {
    public func toSexp() -> String {
        if entries.isEmpty {
            return "(document [-1, -1]\n)"
        }
        let entriesStr = entries.map { $0.toSexp(indent: 1) }.joined(separator: "\n")
        return "(document [-1, -1]\n\(entriesStr)\n)"
    }
}

extension Entry {
    func toSexp(indent: Int) -> String {
        let prefix = String(repeating: "  ", count: indent)
        let keyStr = key.toSexp(indent: indent + 1)
        let valueStr = value.toSexp(indent: indent + 1)
        return "\(prefix)(entry\n\(prefix)  \(keyStr)\n\(prefix)  \(valueStr))"
    }
}

extension Value {
    func toSexp(indent: Int) -> String {
        let prefix = String(repeating: "  ", count: indent)

        // Unit value (no tag, no payload)
        if tag == nil && payload == .none {
            return "(unit [\(span.start), \(span.end)])"
        }

        // Tag only (no payload)
        if let tag = tag, payload == .none {
            return "(tag [\(span.start), \(span.end)] \"\(tag.name)\")"
        }

        // Tag with payload
        if let tag = tag {
            let payloadStr = payloadToSexp(indent: indent + 1)
            return "(tag [\(span.start), \(span.end)] \"\(tag.name)\"\n\(prefix)  \(payloadStr))"
        }

        // Just payload
        return payloadToSexp(indent: indent)
    }

    private func payloadToSexp(indent: Int) -> String {
        let prefix = String(repeating: "  ", count: indent)

        switch payload {
        case .none:
            return "(unit [\(span.start), \(span.end)])"

        case .scalar(let scalar):
            let escaped = escapeString(scalar.text)
            return "(scalar [\(scalar.span.start), \(scalar.span.end)] \(scalar.kind.rawValue) \"\(escaped)\")"

        case .sequence(let seq):
            if seq.items.isEmpty {
                return "(sequence [\(seq.span.start), \(seq.span.end)])"
            }
            let items = seq.items.map { "\(prefix)  \($0.toSexp(indent: indent + 1))" }.joined(separator: "\n")
            return "(sequence [\(seq.span.start), \(seq.span.end)]\n\(items))"

        case .object(let obj):
            if obj.entries.isEmpty {
                return "(object [\(obj.span.start), \(obj.span.end)] \(obj.separator.rawValue))"
            }
            let entries = obj.entries.map { $0.toSexp(indent: indent + 1) }.joined(separator: "\n")
            return "(object [\(obj.span.start), \(obj.span.end)] \(obj.separator.rawValue)\n\(entries)\n\(prefix))"
        }
    }
}

func escapeString(_ s: String) -> String {
    var result = ""
    for char in s {
        switch char {
        case "\\": result += "\\\\"
        case "\"": result += "\\\""
        case "\n": result += "\\n"
        case "\r": result += "\\r"
        case "\t": result += "\\t"
        default: result.append(char)
        }
    }
    return result
}

extension ParseError {
    public func toSexp() -> String {
        let escaped = escapeString(message)
        return "(error [\(span.start), \(span.end)] \"parse error at \(span.start)-\(span.end): \(escaped)\")"
    }
}
