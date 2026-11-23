import Foundation
import SwiftUI

/// Countries as Venti names them, by their ISO 3166-1 alpha-2 codes: a
/// flag drawn from the code and a name in English, as on Android and the
/// web.
///
/// The flag is the pair of regional indicator letters of the code, which
/// the iPhone's own emoji font draws as a flag; no picture of a flag is in
/// the app. The name is `Locale`'s, in English as the rest of the app, and
/// the code itself where it has none.
enum Countries {
    /// The flag of a country from its two letters; `nil` for no code or
    /// one that is not two letters.
    static func flag(_ code: String?) -> String? {
        guard let letters = letters(of: code) else { return nil }
        let scalars = letters.unicodeScalars.compactMap { Unicode.Scalar(regionalA + ($0.value - 65)) }
        return String(String.UnicodeScalarView(scalars))
    }

    /// A country's name in English: "Germany" for `DE`; the code where
    /// `Locale` knows none.
    static func name(_ code: String) -> String {
        let letters = letters(of: code) ?? code.trimmingCharacters(in: .whitespaces).uppercased()
        return english.localizedString(forRegionCode: letters) ?? letters
    }

    /// A country with its flag before its name, "🇩🇪 Germany", for a row
    /// that names it.
    static func named(_ code: String) -> String {
        [flag(code), name(code)].compactMap { $0 }.joined(separator: " ")
    }

    /// Every country `Locale` knows, by its name, for the pilot to choose
    /// from: the regions with two letters, not the continents and other
    /// groups that have three digits.
    static let all: [(code: String, name: String)] = Locale.Region.isoRegions
        .map(\.identifier)
        .filter { letters(of: $0) != nil }
        .map { (code: $0, name: name($0)) }
        .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }

    /// The countries whose name holds the text, or whose code it is.
    static func finding(_ text: String) -> [(code: String, name: String)] {
        let text = text.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return all }
        return all.filter {
            $0.name.localizedStandardContains(text) || $0.code.caseInsensitiveCompare(text) == .orderedSame
        }
    }

    /// The code in capitals where it is two letters A to Z.
    private static func letters(of code: String?) -> String? {
        guard let letters = code?.trimmingCharacters(in: .whitespaces).uppercased(),
              letters.count == 2,
              letters.unicodeScalars.allSatisfy({ (65...90).contains($0.value) })
        else { return nil }
        return letters
    }

    private static let english = Locale(identifier: "en")

    /// The first regional indicator symbol, the letter A.
    private static let regionalA: UInt32 = 0x1F1E6
}

/// A country's flag, which VoiceOver reads as the country's name; nothing
/// without a code.
struct Flag: View {
    let code: String?

    var body: some View {
        if let code, let flag = Countries.flag(code) {
            Text(flag)
                .accessibilityLabel(Countries.name(code))
        }
    }
}
