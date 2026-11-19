package fly.venti.app.map

import fly.venti.app.AppFiles
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import java.io.File

/**
 * Prepares the shared map style for the place its tiles come from.
 *
 * The style in `map/style.json` is the same file for every platform. It
 * leaves open where its two tile sources and its fonts live, because that
 * differs: a region stored on the device, or tiles fetched as the pilot
 * moves the map.
 */
object MapStyle {
    enum class Source {
        /**
         * Tiles fetched from public servers of open map data: OpenFreeMap
         * for the base map, Mapterhorn for terrain. This stands in for the
         * project's own tile server until there is one.
         */
        Online,

        /** The small demo region shipped inside the app. Needs no connection. */
        Bundled,
    }

    /** The public endpoints the online source uses. */
    object Online {
        const val BASEMAP = "https://tiles.openfreemap.org/planet"
        const val TERRAIN_TILES = "https://tiles.mapterhorn.com/{z}/{x}/{y}.webp"

        /**
         * Mapterhorn serves terrain to this zoom everywhere; finer tiles
         * exist only for some countries. Asking for no more keeps the
         * hillshade free of holes.
         */
        const val TERRAIN_MAX_ZOOM = 12
        const val TERRAIN_ATTRIBUTION = "<a href=\"https://mapterhorn.com/attribution\">© Mapterhorn</a>"
    }

    class StyleError(message: String) : Exception(message)

    /**
     * The style as JSON text, pointing at the given source.
     *
     * `template` is the text of the shared style. `region` is the folder on
     * disk that holds the label fonts and the bundled region. Label fonts
     * always come from there, so place names never wait for the network.
     */
    fun json(source: Source, template: String, region: File): String {
        val style = try {
            Json.parseToJsonElement(template) as? JsonObject
        } catch (_: SerializationException) {
            null
        } ?: throw StyleError("The map style is not valid JSON.")
        val sources = style["sources"] as? JsonObject
        val basemap = sources?.get("basemap") as? JsonObject
        val terrain = sources?.get("terrain") as? JsonObject
        if (sources == null || basemap == null || terrain == null) {
            throw StyleError("The map style has no base map or no terrain.")
        }

        val prepared = when (source) {
            Source.Online -> mapOf(
                "basemap" to basemap.with("url" to JsonPrimitive(Online.BASEMAP)),
                "terrain" to JsonObject(terrain - "url").with(
                    "tiles" to JsonArray(listOf(JsonPrimitive(Online.TERRAIN_TILES))),
                    "maxzoom" to JsonPrimitive(Online.TERRAIN_MAX_ZOOM),
                    "attribution" to JsonPrimitive(Online.TERRAIN_ATTRIBUTION),
                ),
            )
            Source.Bundled -> mapOf(
                "basemap" to basemap.with("url" to archive(region, "basemap.pmtiles")),
                "terrain" to terrain.with("url" to archive(region, "terrain.pmtiles")),
            )
        }
        return style.with(
            "glyphs" to JsonPrimitive(url(region.resolve("fonts")) + "/{fontstack}/{range}.pbf"),
            "sources" to JsonObject(sources + prepared),
        ).toString()
    }

    /** A file's location the way the map engine wants it: `file:///data/...`. */
    private fun url(file: File): String = "file://" + file.absolutePath

    /** The address of a tile archive on disk. */
    private fun archive(region: File, name: String) = JsonPrimitive("pmtiles://" + url(region.resolve(name)))

    private fun JsonObject.with(vararg entries: Pair<String, kotlinx.serialization.json.JsonElement>): JsonObject =
        buildJsonObject {
            this@with.forEach { (key, value) -> put(key, value) }
            entries.forEach { (key, value) -> put(key, value) }
        }
}

/**
 * The label fonts and the demo region as files on disk.
 *
 * They ship inside the app, where the map engine cannot read them the way
 * it reads a tile archive: in pieces, by position. So they are copied out
 * once, into a folder the app owns, and the style points there.
 */
object MapRegion {
    /**
     * Makes sure the shipped map files are in `folder` and returns it.
     * `stamp` names the app's version; the files are copied again when it
     * changes, because a new version may ship new ones.
     */
    fun install(files: AppFiles, folder: File, stamp: String): File {
        val marker = folder.resolve("installed")
        if (marker.isFile && marker.readText() == stamp) return folder

        folder.deleteRecursively()
        for (path in files.list(AppFiles.MAP_REGION)) {
            val target = folder.resolve(path.removePrefix(AppFiles.MAP_REGION + "/"))
            target.parentFile?.mkdirs()
            files.open(path).use { input -> target.outputStream().use { input.copyTo(it) } }
        }
        // Written last: a copy that was cut short is made again.
        marker.writeText(stamp)
        return folder
    }
}

/**
 * Text as the map can draw it.
 *
 * The label fonts shipped with the app hold Unicode's first 512 characters:
 * the Latin letters with their accents. A label with any other character,
 * a long dash is enough, makes the map engine ask for a font file that is
 * not there, and the engine then draws nothing at all of what shares that
 * label's source in that part of the map: not the label, and not the lines
 * and areas either. So what the app itself writes on a map goes through
 * here first, and a layer's names are kept apart from its shapes.
 */
object MapText {
    /** The last character the shipped fonts hold. */
    private const val LAST_SHIPPED = 0x1FF

    /** The text with every character the fonts lack replaced by one they have. */
    fun shipped(text: String): String = buildString {
        for (character in text) {
            when {
                character == '\n' -> append(character)
                character.code < 0x20 -> Unit
                character.code <= LAST_SHIPPED -> append(character)
                character in DASHES -> append('-')
                character in SINGLE_QUOTES -> append('\'')
                character in DOUBLE_QUOTES -> append('"')
                character == '\u2026' -> append("...")
                character.isWhitespace() -> append(' ')
                // Half of a character from beyond the first 65,536: the other half says it.
                character.isLowSurrogate() -> Unit
                else -> append('?')
            }
        }
    }

    private const val DASHES = "\u2010\u2011\u2012\u2013\u2014\u2015\u2212"
    private const val SINGLE_QUOTES = "\u2018\u2019\u201A\u2032"
    private const val DOUBLE_QUOTES = "\u201C\u201D\u201E\u2033"
}
