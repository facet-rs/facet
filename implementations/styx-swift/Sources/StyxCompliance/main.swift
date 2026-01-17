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

    var parser = Parser(source: content)

    do {
        let doc = try parser.parse()
        print(doc.toSexp())
    } catch let error as ParseError {
        print(error.toSexp())
    } catch {
        fputs("Error: \(error)\n", stderr)
        exit(1)
    }
}

main()
