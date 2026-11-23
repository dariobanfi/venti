import XCTest

@testable import Venti

/// What "Open-source licences" shows is in the app: the core's notices,
/// MapLibre Native's licence and the typefaces' licence, each cut into
/// paragraphs without losing a word.
final class OpenSourceLicencesTests: XCTestCase {
    func testTheCoresNoticesAreBundled() throws {
        let notices = try XCTUnwrap(OpenSourceLicences.text(OpenSourceLicences.coreNotices))
        XCTAssertTrue(notices.hasPrefix("Third-party software in Venti's core"))
        // The bridge the iPhone app names under its libraries is among
        // the crates, with the version the page gives it.
        let bridge = try XCTUnwrap(OpenSourceLicences.libraries.first { $0.licence == OpenSourceLicences.mpl2 })
        XCTAssertTrue(notices.contains("uniffi \(bridge.version ?? "")\n"))
        XCTAssertTrue(notices.contains("Mozilla Public License"))

        let texts = LicencesScreen.Texts.read()
        XCTAssertGreaterThan(texts.core.count, 100)
        XCTAssertEqual(texts.core.first, LicenceParagraph(text: "Third-party software in Venti's core", isHeading: true))
        XCTAssertTrue(texts.core.contains(LicenceParagraph(text: "Crates", isHeading: true)))
        // A crate is one paragraph of lines, its fields set apart.
        XCTAssertTrue(texts.core.contains { $0.text.hasPrefix("uniffi 0.32.2\n  Licence: MPL-2.0") })
    }

    func testMapLibresLicenceAndTheTypefacesAreBundled() throws {
        let mapLibre = try XCTUnwrap(OpenSourceLicences.text(OpenSourceLicences.mapLibreText))
        XCTAssertTrue(mapLibre.hasPrefix("BSD 2-Clause License"))
        XCTAssertTrue(mapLibre.contains("Copyright (c) 2014-2020 Mapbox"))

        let texts = LicencesScreen.Texts.read()
        XCTAssertEqual(texts.mapLibre.count, 7)
        XCTAssertTrue(texts.mapLibre.contains { $0.text.hasPrefix("THIS SOFTWARE IS PROVIDED") && !$0.text.contains("\n") })
        XCTAssertTrue(texts.typeface.contains { $0.text.contains("Mozilla Foundation and Telefonica") })
        XCTAssertEqual(texts.labelsCopyright.map(\.text), ["Copyright 2022 The Noto Project Authors (https://github.com/notofonts)"])
    }

    /// The version on the page is the one the project pins, and the file is
    /// the package's own where the build has checked the package out.
    func testMapLibresVersionAndFileAreThePackages() throws {
        let apps = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        let project = try String(contentsOf: apps.appending(path: "project.yml"), encoding: .utf8)
        XCTAssertTrue(project.contains("exactVersion: \(OpenSourceLicences.mapLibre.version ?? "")"))

        let checkout = apps.appending(path: "../../target/apple/DerivedData/SourcePackages/checkouts/maplibre-gl-native-distribution/LICENSE.md")
        if let package = try? Data(contentsOf: checkout) {
            let bundled = try XCTUnwrap(Bundle.main.url(forResource: OpenSourceLicences.mapLibreText, withExtension: "txt"))
            XCTAssertEqual(try Data(contentsOf: bundled), package)
        }
    }

    /// Lines wrapped inside a sentence are joined; a list's items, a
    /// title underlined and copyright lines keep their own lines.
    func testALicenceIsCutIntoParagraphs() {
        let text = """
            A Title
            =======

            Redistribution and use in source and binary forms, with or without
            modification, are permitted provided that the following conditions are
            met:

            * Redistributions of source code must retain the above copyright
              notice, this list of conditions and the following disclaimer.
            * Redistributions in binary form must reproduce the above copyright
              notice.

            Copyright (c) 2021 Someone
            Copyright (c) 2014 Someone else
            """
        XCTAssertEqual(LicenceText.paragraphs(text), [
            LicenceParagraph(text: "A Title", isHeading: true),
            LicenceParagraph(text: "Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:"),
            LicenceParagraph(text: "* Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.\n* Redistributions in binary form must reproduce the above copyright notice."),
            LicenceParagraph(text: "Copyright (c) 2021 Someone\nCopyright (c) 2014 Someone else"),
        ])
    }
}
