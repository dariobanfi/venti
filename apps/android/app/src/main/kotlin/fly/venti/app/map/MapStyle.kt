package fly.venti.app.map

import fly.venti.app.maps.KeptMap
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

    /** The elevation tiles of the region that comes with the app, by their file's name in it. */
    const val BUNDLED_TERRAIN = "terrain.pmtiles"

    class StyleError(message: String) : Exception(message)

    /**
     * The style as JSON text, pointing at the given source.
     *
     * `template` is the text of the shared style. `region` is the folder on
     * disk that holds the label fonts and the bundled region. Label fonts
     * always come from there, so place names never wait for the network.
     */
    fun json(source: Source, template: String, region: File, kept: KeptMap? = null): String {
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

        val prepared = when {
            // A region downloaded for flying without a network: its two archives on the phone.
            kept != null -> mapOf(
                "basemap" to basemap.with("url" to JsonPrimitive("pmtiles://" + url(kept.basemap))),
                "terrain" to terrain.with("url" to JsonPrimitive("pmtiles://" + url(kept.terrain))),
            )
            source == Source.Online -> mapOf(
                "basemap" to basemap.with("url" to JsonPrimitive(Online.BASEMAP)),
                "terrain" to JsonObject(terrain - "url").with(
                    "tiles" to JsonArray(listOf(JsonPrimitive(Online.TERRAIN_TILES))),
                    "maxzoom" to JsonPrimitive(Online.TERRAIN_MAX_ZOOM),
                    "attribution" to JsonPrimitive(Online.TERRAIN_ATTRIBUTION),
                ),
            )
            else -> mapOf(
                "basemap" to basemap.with("url" to archive(region, "basemap.pmtiles")),
                "terrain" to terrain.with("url" to archive(region, BUNDLED_TERRAIN)),
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
