package fly.venti.app.settings

import fly.venti.app.AppFiles
import fly.venti.app.TestSupport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** What "Open-source licences" shows: the core's notices, the licence texts, the libraries' versions. */
class OpenSourceLicencesTest {
    @Test
    fun theCoreNoticesComeWithTheApp() {
        val notices = TestSupport.files.text(AppFiles.CORE_NOTICES)
        assertTrue(notices.isNotBlank())
        assertTrue(notices.startsWith("Third-party software in Venti's core"))
        val paragraphs = LicenceText.paragraphs(notices)
        assertEquals(LicenceParagraph("Third-party software in Venti's core", isHeading = true), paragraphs.first())
        assertTrue(LicenceParagraph("Crates", isHeading = true) in paragraphs)
        // A crate keeps its fields on lines of their own.
        val crate = paragraphs.first { it.text.startsWith("anyhow ") }
        assertTrue(crate.text, crate.text.lines().any { it.trim().startsWith("Source: https://crates.io/crates/anyhow/") })
        assertTrue(crate.text, crate.text.lines().any { it.trim().startsWith("No copyright line") })
    }

    @Test
    fun theLicenceTextsAreTheLibrariesOwn() {
        val assets = File("src/main/assets")
        val apache = assets.resolve(OpenSourceLicences.APACHE_TEXT).readText()
        assertTrue(apache.contains("Apache License\n                           Version 2.0, January 2004"))
        assertTrue(apache.trimEnd().endsWith("END OF TERMS AND CONDITIONS"))
        val mapLibre = assets.resolve(OpenSourceLicences.MAPLIBRE_TEXT).readText()
        for (line in listOf(
            "Copyright (c) 2021 MapLibre contributors",
            "Copyright (c) 2018-2021 MapTiler.com",
            "Copyright (c) 2014-2020 Mapbox",
        )) {
            assertTrue(line, mapLibre.contains(line))
        }
        assertTrue(assets.resolve(OpenSourceLicences.BSD_3_TEXT).readText().contains("Neither the name of the copyright holder"))
    }

    @Test
    fun aLineBrokenInsideASentenceIsJoinedAndAListKeepsItsItems() {
        val text = """
            Redistribution and use in source and binary forms, with or without
            modification, are permitted provided that the following conditions are
            met:

            * Redistributions of source code must retain the above copyright
              notice, this list of conditions and the following disclaimer.
            * Redistributions in binary form must reproduce the above copyright
              notice.

            Licence texts
            -------------
        """.trimIndent()
        val paragraphs = LicenceText.paragraphs(text)
        assertEquals(
            "Redistribution and use in source and binary forms, with or without modification, are permitted " +
                "provided that the following conditions are met:",
            paragraphs[0].text,
        )
        assertEquals(
            "* Redistributions of source code must retain the above copyright notice, this list of conditions and " +
                "the following disclaimer.\n* Redistributions in binary form must reproduce the above copyright notice.",
            paragraphs[1].text,
        )
        assertEquals(LicenceParagraph("Licence texts", isHeading = true), paragraphs[2])
        assertFalse(paragraphs.any { it.text.contains("---") })
    }

    @Test
    fun everyLibraryOfTheCatalogHasItsVersion() {
        assertNotNull(OpenSourceLicences.mapLibre.version)
        assertEquals(OpenSourceLicences.versions["maplibre"], OpenSourceLicences.mapLibre.version)
        for (name in listOf("CameraX", "Kotlin", "JNA", "ZXing", "Firebase Cloud Messaging", "Sign in with Google")) {
            val library = OpenSourceLicences.libraries.first { it.name == name }
            assertNotNull(name, library.version)
        }
    }
}
