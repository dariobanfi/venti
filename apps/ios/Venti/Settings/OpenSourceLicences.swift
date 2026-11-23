import Foundation

/// What "Open-source licences" lists: the software inside the iPhone app
/// besides Venti's own, with the licence each is under.
///
/// The core's crates are in `core-notices.txt`, `licences/core-notices.txt`
/// at the repository's root, generated from Cargo.lock and bundled by
/// `project.yml` as it is. MapLibre Native's licence is
/// `Resources/Licences/MapLibre-Native.txt`, the `LICENSE.md` of the
/// package maplibre-gl-native-distribution 7.0.0 byte for byte (the
/// same file as Android's). The typeface's licence is the pack's `OFL.txt`,
/// which the app carries beside the fonts; the map's labels have theirs in
/// `MapDemo/fonts/OFL.txt`.
enum OpenSourceLicences {
    static let coreNotices = "core-notices"
    static let mapLibreText = "MapLibre-Native"
    static let typefaceText = "OFL"

    static let bsd2 = "BSD-2-Clause"
    static let mpl2 = "MPL-2.0"
    static let ofl = "OFL-1.1"

    /// The map engine, with the version `project.yml` pins.
    static let mapLibre = Library(name: "MapLibre Native", version: "7.0.0", licence: bsd2)

    /// What the iPhone app carries besides the core and the map engine,
    /// in the order the page lists them. Apple's frameworks are part of
    /// the iPhone's system and not carried.
    static let libraries: [Library] = [
        // The Swift the bridge generates into the core's package; the
        // crate's version and the MPL's text are in the core's notices.
        Library(name: "UniFFI's Swift bridge, in Venti's core package", version: "0.32.2", licence: mpl2),
        Library(name: "Fira Sans, the app's typeface", version: nil, licence: ofl),
        Library(name: "Noto Sans, the names on the map that comes with the app", version: nil, licence: ofl),
    ]

    /// One library: its name, the version it is built with, and the
    /// licence it is used under.
    struct Library: Hashable, Sendable {
        let name: String
        let version: String?
        let licence: String

        /// The line under the name: version and licence.
        var line: String {
            [version, licence].compactMap { $0 }.joined(separator: " · ")
        }
    }

    /// A text file of the app's bundle, or nil where it is not there.
    static func text(_ name: String, in folder: String? = nil, bundle: Bundle = .main) -> String? {
        guard let url = bundle.url(forResource: name, withExtension: "txt", subdirectory: folder) else {
            return nil
        }
        return try? String(contentsOf: url, encoding: .utf8)
    }
}

/// The page's words, Android's with "iPhone" for "Android". Not the
/// core's: the core has none for it.
enum LicenceWords {
    static let title = "Open-source licences"
    static let core = "Venti's core"
    static let mapEngine = "MapLibre Native"
    static let libraries = "iPhone libraries"
    static let typeface = "SIL Open Font License 1.1"
}

/// A piece of a licence file as the page shows it: a heading, or a paragraph.
struct LicenceParagraph: Hashable, Sendable {
    let text: String
    var isHeading = false
}

/// A licence file in pieces a phone can show, as Android's `LicenceText`
/// cuts it. The files are wrapped at about 72 characters, which on a
/// narrow screen with large text wrap again into ragged lines; so a line
/// break inside a sentence is taken out and the paragraph left to wrap at
/// the screen's width. The words stay as they are: only breaks and
/// indentation change. A line of `=` or `-` under a title makes the title
/// a heading, and is dropped.
enum LicenceText {
    static func paragraphs(_ text: String) -> [LicenceParagraph] {
        let normal = text.replacingOccurrences(of: "\r\n", with: "\n")
        return normal.split(separator: /\n[ \t]*\n/, omittingEmptySubsequences: false).compactMap { block in
            let lines = block.split(separator: "\n", omittingEmptySubsequences: false)
                .map(String.init)
                .filter { !trimmed($0).isEmpty }
            let words = lines.filter { !isRule($0) }
            if words.isEmpty {
                return nil
            }
            if words.count == 1, lines.count > 1 {
                return LicenceParagraph(text: trimmed(words[0]), isHeading: true)
            }
            return LicenceParagraph(text: join(words))
        }
    }

    /// A list's item: `*`, `-`, `1.`, `1.1.`, `(a)`, `a)`.
    private static var item: Regex<Substring> { /^(?:[*•-]\s|\d+(?:\.\d+)*\.?\s|\(?[a-z0-9]{1,4}\)\s)/ }

    /// A field of the core's list: `Licence:`, `Source:`, `Repository:`.
    private static var field: Regex<Substring> { /^[A-Z][A-Za-z -]{0,24}:\s/ }

    private static func trimmed(_ line: String) -> String {
        line.trimmingCharacters(in: .whitespaces)
    }

    private static func isRule(_ line: String) -> Bool {
        let rule = trimmed(line)
        return rule.count >= 3 && rule.allSatisfy { $0 == "=" || $0 == "-" }
    }

    private static func indent(_ line: String) -> Int {
        line.prefix { $0.isWhitespace }.count
    }

    /// Whether `line` goes on the sentence of `previous` rather than
    /// starting a line of its own.
    private static func continues(_ previous: String, _ line: String) -> Bool {
        let next = trimmed(line)
        let before = previous.replacing(/\s+$/, with: "")
        return trimmed(previous).count >= 40
            && indent(line) >= indent(previous)
            // A command or a quotation set in under a colon.
            && !(before.hasSuffix(":") && indent(line) > indent(previous))
            && next.firstMatch(of: item) == nil
            && next.firstMatch(of: field) == nil
            && !next.hasPrefix("Copyright")
            && !next.hasPrefix("No copyright")
            && !next.hasPrefix("©")
            && !next.hasPrefix("http")
    }

    private static func join(_ lines: [String]) -> String {
        let margin = lines.map(indent).min() ?? 0
        // A field of the core's list keeps its few spaces; a title centred
        // with many is set flush.
        func start(_ line: String) -> String {
            let shifted = String(line.dropFirst(margin))
            let flush = indent(shifted) > 4 ? String(shifted.drop { $0.isWhitespace }) : shifted
            return flush.replacing(/\s+$/, with: "")
        }
        var out = start(lines[0])
        for index in lines.indices.dropFirst() {
            if continues(lines[index - 1], lines[index]) {
                out += " " + trimmed(lines[index])
            } else {
                out += "\n" + start(lines[index])
            }
        }
        return out
    }
}
