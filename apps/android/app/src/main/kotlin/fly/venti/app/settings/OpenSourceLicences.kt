package fly.venti.app.settings

import fly.venti.app.AppFiles
import fly.venti.app.BuildConfig

/**
 * What "Open-source licences" lists: the software inside the Android app
 * besides Venti's own, with the licence each is under.
 *
 * The core's crates are in `licences/core-notices.txt`, generated from
 * Cargo.lock and bundled by the build; the texts of the other licences are
 * in the app's assets, `licences/`, each taken byte for byte from a file
 * the libraries publish (read from the Gradle cache on 2026-11-23):
 * `Apache-2.0.txt` is `META-INF/AL2.0` of JNA 5.19.1's jar,
 * `BSD-3-Clause.txt` the `LICENSE.txt` that androidx.glance's
 * glance-appwidget-external-protobuf 1.2.0 carries for Protocol Buffers,
 * and `MapLibre-Native.txt` MapLibre Native's `LICENSE.md`. MapLibre's
 * Android archive carries no licence file of its own; that file is the one
 * MapLibre ships with its iPhone distribution (the same project): to verify
 * against the maplibre-native repository at the tag android-v13.7.1.
 *
 * Each library's licence is the one its POM names. The versions are the
 * version catalog's (`gradle/libs.versions.toml`), handed over by the build
 * in `BuildConfig.LIBRARY_VERSIONS`; a library the catalog does not name
 * comes in with another and is listed without a version.
 */
object OpenSourceLicences {
    const val APACHE_TEXT = "licences/Apache-2.0.txt"
    const val BSD_3_TEXT = "licences/BSD-3-Clause.txt"
    const val MAPLIBRE_TEXT = "licences/MapLibre-Native.txt"
    const val CORE_NOTICES = AppFiles.CORE_NOTICES

    /**
     * The notices Google's libraries carry for what is inside them, gathered
     * by the build's `googleNotices` task from their `third_party_licenses`
     * files; never committed.
     */
    const val GOOGLE_NOTICES = "licences/google-third-party.txt"

    const val APACHE = "Apache-2.0"
    const val BSD_2 = "BSD-2-Clause"
    const val BSD_3 = "BSD-3-Clause"
    const val SDK_LICENSE = "Android Software Development Kit License"

    /** Where Google's licence for its SDK libraries stands; its text does not come with them. */
    const val SDK_LICENSE_LINK = "https://developer.android.com/studio/terms.html"

    /** Every version of the catalog by its alias, such as `camera` or `compose.bom`. */
    val versions: Map<String, String> = parseVersions(BuildConfig.LIBRARY_VERSIONS)

    fun parseVersions(field: String): Map<String, String> =
        field.split(',').filter { '=' in it }.associate { it.substringBefore('=') to it.substringAfter('=') }

    /** The map engine, with its version. */
    val mapLibre = Library("MapLibre Native", versions["maplibre"], BSD_2)

    /** The libraries of the Android app, in the order the page lists them. */
    val libraries: List<Library> = listOf(
        Library("Jetpack Compose and Material 3", versions["compose.bom"]?.let { "BOM $it" }, APACHE),
        Library("AndroidX Core", versions["core"], APACHE),
        Library("AndroidX Activity", versions["activity"], APACHE),
        Library("WorkManager", versions["work"], APACHE),
        Library("Glance", versions["glance"], APACHE),
        Library("Credential Manager", versions["credentials"], APACHE),
        // camera-core's POM names a BSD licence beside Apache-2.0 for the
        // libyuv it carries, and points at libyuv's README.chromium; that
        // licence's text is not in the cache: to verify and add.
        Library("CameraX", versions["camera"], "$APACHE; libyuv inside it: BSD"),
        Library("ExifInterface", versions["exifinterface"], APACHE),
        Library("Kotlin", versions["kotlin"], APACHE),
        Library("kotlinx.serialization", versions["serialization.json"], APACHE),
        // JNA's POM offers LGPL-2.1-or-later or Apache-2.0.
        Library("JNA", versions["jna"], "$APACHE, of $APACHE or LGPL-2.1"),
        Library("ZXing", versions["zxing"], APACHE),
        Library("Firebase Cloud Messaging", versions["firebase.messaging"], APACHE),
        // What the libraries above bring with them, by the POMs of
        // `:app:dependencies --configuration releaseRuntimeClasspath` on
        // 2026-11-23: more AndroidX libraries, kotlinx.coroutines, Okio,
        // JSpecify, Error Prone's and AutoValue's annotations, Guava's
        // listenablefuture, javax.inject, JetBrains' annotations, Firebase's
        // other libraries and Google's data transport.
        Library("AndroidX, Kotlin, Firebase and Google libraries these bring", null, APACHE),
        Library("MapLibre GeoJSON and Turf, inside MapLibre Native", null, APACHE),
        // Its own LICENSE is not in the cache, so its copyright lines are
        // not shown: to verify (github.com/maplibre/maplibre-gestures-android).
        Library("MapLibre gestures, inside MapLibre Native", null, BSD_2),
        Library("OkHttp, Okio, Gson and Timber, inside MapLibre Native", null, APACHE),
        // OkHttp's NOTICE: its public suffix list is under MPL-2.0, whose
        // text the core's notices carry.
        Library("OkHttp's public suffix list", null, "MPL-2.0"),
        // The copyright line of Protocol Buffers is not in the file the
        // libraries carry: to verify.
        Library("Protocol Buffers, inside Glance and DataStore", null, BSD_3),
        // Not open source. The notices of what is inside them (Guava,
        // Protocol Buffers, the Checker Framework …) are the page's fourth
        // part, [GOOGLE_NOTICES].
        Library("Sign in with Google", versions["googleid"], SDK_LICENSE),
        Library("Google Play services, which Credential Manager and Firebase bring", null, SDK_LICENSE),
    )

    /** One library: its name, the version it is built with, and the licence it is used under. */
    data class Library(val name: String, val version: String?, val licence: String) {
        /** The line under the name: version and licence. */
        val line: String get() = listOfNotNull(version, licence).joinToString(" · ")
    }
}

/** The page's words. Not the core's: the core has none for it. */
object LicenceWords {
    const val TITLE = "Open-source licences"
    const val CORE = "Venti's core"
    const val MAP_ENGINE = "MapLibre Native"
    const val LIBRARIES = "Android libraries"
    const val GOOGLE = "Bundled by Google's libraries"
    const val APACHE = "Apache License 2.0"
    const val BSD_3 = "BSD 3-Clause License"
    const val SDK_LICENSE = "Android Software Development Kit License"
}

/** A piece of a licence file as the page shows it: a heading, or a paragraph. */
data class LicenceParagraph(val text: String, val isHeading: Boolean = false)

/**
 * A licence file in pieces a phone can show. The files are wrapped at
 * about 72 characters, which on a narrow screen with large text wrap again
 * into ragged lines; so a line break inside a sentence is taken out and the
 * paragraph left to wrap at the screen's width. The words stay as they
 * are: only breaks and indentation change. A line of `=` or `-` under a
 * title makes the title a heading, and is dropped.
 */
object LicenceText {
    fun paragraphs(text: String): List<LicenceParagraph> =
        text.replace("\r\n", "\n").split(BLANK_LINE).mapNotNull { block ->
            val lines = block.lines().filter { it.isNotBlank() }
            val words = lines.filterNot(::isRule)
            when {
                words.isEmpty() -> null
                words.size == 1 && lines.size > 1 -> LicenceParagraph(words.single().trim(), isHeading = true)
                else -> LicenceParagraph(join(words))
            }
        }

    private val BLANK_LINE = Regex("\n[ \t]*\n")

    /** A list's item: `*`, `-`, `1.`, `1.1.`, `(a)`, `a)`. */
    private val ITEM = Regex("""^([*•-]\s|\d+(\.\d+)*\.?\s|\(?[a-z0-9]{1,4}\)\s)""")

    /** A field of the core's list: `Licence:`, `Source:`, `Repository:`. */
    private val FIELD = Regex("""^[A-Z][A-Za-z -]{0,24}:\s""")

    private fun isRule(line: String): Boolean = line.trim().let { it.length >= 3 && it.all { c -> c == '=' || c == '-' } }

    private fun indent(line: String): Int = line.indexOfFirst { !it.isWhitespace() }.coerceAtLeast(0)

    /** Whether `line` goes on the sentence of `previous` rather than starting a line of its own. */
    private fun continues(previous: String, line: String): Boolean {
        val next = line.trim()
        return previous.trim().length >= 40 &&
            indent(line) >= indent(previous) &&
            // A command or a quotation set in under a colon.
            !(previous.trimEnd().endsWith(":") && indent(line) > indent(previous)) &&
            !ITEM.containsMatchIn(next) &&
            !FIELD.containsMatchIn(next) &&
            !next.startsWith("Copyright") &&
            !next.startsWith("No copyright") &&
            !next.startsWith("©") &&
            !next.startsWith("http")
    }

    private fun join(lines: List<String>): String {
        val margin = lines.minOf(::indent)
        // A field of the core's list keeps its few spaces; a title centred
        // with many is set flush.
        fun start(line: String) = line.drop(margin).let { if (indent(it) > 4) it.trimStart() else it }.trimEnd()
        val out = StringBuilder(start(lines.first()))
        for (i in 1 until lines.size) {
            if (continues(lines[i - 1], lines[i])) {
                out.append(' ').append(lines[i].trim())
            } else {
                out.append('\n').append(start(lines[i]))
            }
        }
        return out.toString()
    }
}
