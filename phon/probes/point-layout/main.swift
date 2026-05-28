import Foundation

struct Point {
    var x: UInt32
    var y: Double
}

struct PointReordered {
    var y: Double
    var x: UInt32
}

struct Field {
    let name: String
    let offset: Int
    let size: Int
}

func printLayout<T>(_ type: T.Type, name: String, fields: [Field]) {
    let stride = MemoryLayout<T>.stride
    let align = MemoryLayout<T>.alignment

    print("// print-type-size type: `\(name)`: \(stride) bytes, alignment: \(align) bytes")

    let sorted = fields.sorted { $0.offset < $1.offset }
    var cursor = 0
    for f in sorted {
        if f.offset > cursor {
            print("// print-type-size     padding: \(f.offset - cursor) bytes")
        }
        print("// print-type-size     field `.\(f.name)`: \(f.size) bytes")
        cursor = f.offset + f.size
    }
    if stride > cursor {
        print("// print-type-size     end padding: \(stride - cursor) bytes")
    }
}

printLayout(Point.self, name: "Point", fields: [
    Field(name: "x",
          offset: MemoryLayout<Point>.offset(of: \Point.x)!,
          size: MemoryLayout<UInt32>.size),
    Field(name: "y",
          offset: MemoryLayout<Point>.offset(of: \Point.y)!,
          size: MemoryLayout<Double>.size),
])

print("")

printLayout(PointReordered.self, name: "PointReordered", fields: [
    Field(name: "y",
          offset: MemoryLayout<PointReordered>.offset(of: \PointReordered.y)!,
          size: MemoryLayout<Double>.size),
    Field(name: "x",
          offset: MemoryLayout<PointReordered>.offset(of: \PointReordered.x)!,
          size: MemoryLayout<UInt32>.size),
])
