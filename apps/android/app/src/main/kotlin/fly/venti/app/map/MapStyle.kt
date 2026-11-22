package fly.venti.app.map

import fly.venti.core.Bounds
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
    /** Where the style names Venti itself, for what it asks of Venti: the photographs of France and Austria. */
    const val SERVER_PLACEHOLDER = "__VENTI_SERVER__"

    /** Where the style names the app's own domain, to the servers that ask a client to say who it is. */
    const val DOMAIN_PLACEHOLDER = "__APP_DOMAIN__"

    /**
     * The app's own domain, which a tile address that asks for it is given.
     * `null`: Venti has no domain yet, and nothing that asks for one is
     * fetched without it; `docs/data-sources.md` says why for each source.
     */
    val APP_DOMAIN: String? = null

    /**
     * `server` is Venti's address, the app's, without a slash
     * at its end, and `appDomain` the app's own domain. A tile address that
     * names one of them is given it; one that names a placeholder without a
     * value is left out, with the layers that draw from it.
     */
    fun json(
        source: Source,
        template: String,
        region: File,
        kept: KeptMap? = null,
        server: String? = null,
        /** The app's own domain, for a tile address that asks for it; [APP_DOMAIN] in the app. */
        appDomain: String? = APP_DOMAIN,
    ): String {
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
        // The placeholders in tile addresses, each with its value or none.
        // A source that names one without a value is left out, with every
        // layer that draws from it: nothing is asked without it.
        // A region kept for flying without a network asks nobody for more:
        // nothing that needs the app's domain, the thermal map, is fetched for it.
        val values = mapOf(SERVER_PLACEHOLDER to server, DOMAIN_PLACEHOLDER to appDomain.takeIf { kept == null })
        fun tilesOf(described: kotlinx.serialization.json.JsonElement): List<String> =
            ((described as? JsonObject)?.get("tiles") as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.content }
        val left = mutableSetOf<String>()
        val filled = (sources + prepared).mapNotNull { (name, described) ->
            val tiles = tilesOf(described)
            val named = values.keys.filter { placeholder -> tiles.any { placeholder in it } }
            when {
                named.isEmpty() -> name to described
                named.any { values[it] == null } -> {
                    left += name
                    null
                }
                else -> name to (described as JsonObject).with(
                    "tiles" to JsonArray(
                        tiles.map { tile -> JsonPrimitive(named.fold(tile) { text, placeholder -> text.replace(placeholder, values[placeholder].orEmpty()) }) },
                    ),
                )
            }
        }.toMap()
        val layers = (style["layers"] as? JsonArray)?.let { all ->
            if (left.isEmpty()) {
                all
            } else {
                JsonArray(all.filter { layer -> (((layer as? JsonObject)?.get("source")) as? JsonPrimitive)?.content !in left })
            }
        }
        return style.with(
            "glyphs" to JsonPrimitive(url(region.resolve("fonts")) + "/{fontstack}/{range}.pbf"),
            "sources" to JsonObject(filled),
            *listOfNotNull(layers?.let { "layers" to it }).toTypedArray(),
        ).toString()
    }

    /**
     * The layers that belong to one of the map's two views only: the drawn
     * ground, or the aerial view, photographs from above. Every other layer
     * is in both.
     */
    data class Views(
        val aerial: List<String>,
        val drawn: List<String>,
        /** The layers of the thermal map, marked `venti:layer: thermals`: switched by their own chip. */
        val thermals: List<String> = emptyList(),
    )

    /**
     * Which layers of the shared style belong to which view. The style says
     * so itself, as `venti:view` in a layer's metadata, and stands in the
     * drawn view: the aerial view's layers are hidden in the file.
     */
    fun views(template: String): Views {
        val layers = (
            try {
                Json.parseToJsonElement(template) as? JsonObject
            } catch (_: SerializationException) {
                null
            }
            )?.get("layers") as? JsonArray ?: throw StyleError("The map style has no layers.")
        fun names(key: String, value: String): List<String> = layers.mapNotNull { layer ->
            val fields = layer as? JsonObject ?: return@mapNotNull null
            val metadata = fields["metadata"] as? JsonObject
            if ((metadata?.get(key) as? JsonPrimitive)?.content == value) {
                (fields["id"] as? JsonPrimitive)?.content
            } else {
                null
            }
        }
        return Views(
            aerial = names("venti:view", "aerial"),
            drawn = names("venti:view", "drawn"),
            thermals = names("venti:layer", "thermals"),
        )
    }

    /**
     * One layer of photographs of the aerial view, as the short credit line
     * needs it: whose they are (`venti:credit`), where its source has
     * tiles (`bounds`; `null` for everywhere) and from which zoom the layer
     * shows (the higher `minzoom` of the layer and its source; 0 without one).
     */
    data class AerialPhotos(val credit: String, val bounds: Bounds?, val minZoom: Double)

    /**
     * The layers of photographs of a prepared style, in the style's order:
     * those marked `venti:view: aerial`, of type raster, with a
     * `venti:credit`, whose source the style has. A source left out of the
     * prepared style (one that asks a server the app has not got) is left
     * out with its layer.
     */
    fun aerialPhotos(style: String): List<AerialPhotos> {
        val fields = (
            try {
                Json.parseToJsonElement(style) as? JsonObject
            } catch (_: SerializationException) {
                null
            }
            ) ?: return emptyList()
        val sources = fields["sources"] as? JsonObject ?: return emptyList()
        val layers = fields["layers"] as? JsonArray ?: return emptyList()
        return layers.mapNotNull { element ->
            val layer = element as? JsonObject ?: return@mapNotNull null
            val metadata = layer["metadata"] as? JsonObject ?: return@mapNotNull null
            if ((metadata["venti:view"] as? JsonPrimitive)?.content != "aerial") return@mapNotNull null
            if ((layer["type"] as? JsonPrimitive)?.content != "raster") return@mapNotNull null
            val credit = (metadata["venti:credit"] as? JsonPrimitive)?.content ?: return@mapNotNull null
            val source = sources[(layer["source"] as? JsonPrimitive)?.content] as? JsonObject ?: return@mapNotNull null
            val box = (source["bounds"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.content?.toDoubleOrNull() }
            AerialPhotos(
                credit = credit,
                bounds = box?.takeIf { it.size == 4 }?.let { (west, south, east, north) -> Bounds(south, west, north, east) },
                // A source with a minzoom of its own (France's) has no tiles
                // below it, whatever the layer's: the higher of the two.
                minZoom = maxOf(zoomIn(layer), zoomIn(source)),
            )
        }
    }

    private fun zoomIn(described: JsonObject): Double =
        (described["minzoom"] as? JsonPrimitive)?.content?.toDoubleOrNull() ?: 0.0

    /**
     * The short line that names the makers of the photographs in view:
     * "Photos: " and the credit of every layer whose bounds meet the part
     * of the map on screen and whose `minzoom` the zoom has reached. A
     * layer without bounds counts everywhere and comes last; each name
     * once, " · " between. `null` when none is in view.
     */
    fun photosCredit(layers: List<AerialPhotos>, visible: Bounds, zoom: Double): String? {
        val shown = layers.filter { layer ->
            zoom >= layer.minZoom && layer.bounds.let { it == null || meet(it, visible) }
        }
        val names = (shown.filter { it.bounds != null } + shown.filter { it.bounds == null }).map { it.credit }.distinct()
        return if (names.isEmpty()) null else "Photos: " + names.joinToString(" · ")
    }

    /** Whether two boxes have some part in common, edges included. */
    private fun meet(one: Bounds, other: Bounds): Boolean =
        one.south <= other.north && other.south <= one.north && one.west <= other.east && other.west <= one.east

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
