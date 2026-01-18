import Foundation
import Styx

func main() {
    let args = CommandLine.arguments

    if args.count < 2 {
        fputs("Usage: styx-compliance <file.styx>\n", stderr)
        exit(1)
    }

    let filePath = args[1]

    guard let content = try? String(contentsOfFile: filePath, encoding: .utf8) else {
        fputs("Error: could not read file: \(filePath)\n", stderr)
        exit(1)
    }

    // Extract relative path for comment (compliance/corpus/...)
    let relativePath: String
    if let corpusRange = filePath.range(of: "compliance/corpus/") {
        relativePath = String(filePath[corpusRange.lowerBound...])
    } else {
        relativePath = filePath
    }

    var parser = Parser(source: content)

    do {
        let doc = try parser.parse()
        print("; file: \(relativePath)")
        print(doc.toSexp())
    } catch let error as ParseError {
        print("; file: \(relativePath)")
        print(error.toSexp())
    } catch {
        fputs("Error: \(error)\n", stderr)
        exit(1)
    }
}

main()
