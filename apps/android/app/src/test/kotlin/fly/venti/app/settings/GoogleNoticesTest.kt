package fly.venti.app.settings

import fly.venti.app.TestSupport
import fly.venti.build.GoogleNotices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/**
 * How the build gathers the notices Google's libraries carry
 * (`googleNotices`, with `GoogleNotices` from buildSrc), on made-up
 * archives laid out as Google's are: the texts one after the other, and an
 * index of where each lies, in bytes.
 */
class GoogleNoticesTest {
    private val folder = TestSupport.newFolder("google-notices")

    /** An archive with the two files at its root, its texts at the offsets an index gives. */
    private fun aar(name: String, vararg notices: Pair<String, String>): File {
        val text = StringBuilder()
        val index = mutableListOf<String>()
        for ((title, body) in notices) {
            text.append("\n\n").append(title).append(":\n\n")
            val start = text.toString().toByteArray().size
            text.append(body)
            index += "\"$title\": {\"length\": ${body.toByteArray().size}, \"start\": $start}"
        }
        val file = folder.resolve(name)
        ZipOutputStream(file.outputStream()).use { zip ->
            zip.putNextEntry(ZipEntry("AndroidManifest.xml"))
            zip.write("<manifest/>".toByteArray())
            zip.putNextEntry(ZipEntry(GoogleNotices.TEXTS))
            zip.write(text.toString().toByteArray())
            zip.putNextEntry(ZipEntry(GoogleNotices.INDEX))
            zip.write(index.joinToString(", ", "{", "}").toByteArray())
        }
        return file
    }

    @Test
    fun eachNoticeIsReadAtItsOffsetInBytes() {
        // A letter beyond ASCII before the second text: an offset counted
        // in characters would cut it wrong.
        val file = aar(
            "play-services-made-up.aar",
            "J2ObjC" to "Copyright 2009 Google Inc. — ©\nLicensed under the Apache License.",
            "Animal Sniffer" to "The MIT License\n\nCopyright (c) 2008 Kohsuke Kawaguchi",
        )
        val notices = GoogleNotices.read(file)
        assertEquals(listOf("J2ObjC", "Animal Sniffer"), notices.map { it.name })
        assertEquals("Copyright 2009 Google Inc. — ©\nLicensed under the Apache License.", notices[0].text)
        assertEquals("The MIT License\n\nCopyright (c) 2008 Kohsuke Kawaguchi", notices[1].text)
    }

    @Test
    fun anArchiveWithoutNoticesGivesNone() {
        val file = folder.resolve("plain.aar")
        ZipOutputStream(file.outputStream()).use { zip ->
            zip.putNextEntry(ZipEntry("classes.jar"))
            zip.write(byteArrayOf(1, 2, 3))
        }
        assertTrue(GoogleNotices.read(file).isEmpty())
    }

    @Test
    fun oneEntryForEachNameSortedWithEachDifferentTextOnce() {
        val notices = GoogleNotices.read(aar("a.aar", "Kotlin" to "Kotlin, first text", "Dagger" to "Dagger's text")) +
            GoogleNotices.read(aar("b.aar", "Kotlin" to "Kotlin, first text", "Animal Sniffer" to "MIT")) +
            GoogleNotices.read(aar("c.aar", "Kotlin" to "Kotlin, second text"))
        assertEquals(3, GoogleNotices.count(notices))
        val written = GoogleNotices.write(notices)
        assertEquals(
            "Animal Sniffer\n--------------\n\nMIT\n\n" +
                "Dagger\n------\n\nDagger's text\n\n" +
                "Kotlin\n------\n\nKotlin, first text\n\nKotlin, second text\n\n",
            written,
        )
        // The page shows each name as a heading over its text.
        val paragraphs = LicenceText.paragraphs(written)
        assertEquals(LicenceParagraph("Animal Sniffer", isHeading = true), paragraphs[0])
        assertEquals(LicenceParagraph("MIT"), paragraphs[1])
        assertEquals(LicenceParagraph("Kotlin", isHeading = true), paragraphs[4])
    }
}
