package fly.venti.app.map

import fly.venti.app.AppFiles
import fly.venti.app.TestSupport
import fly.venti.core.Bounds
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Checks that the style is prepared correctly for both places tiles can
 * come from, and that the map data shipped with the app is complete.
 */
class MapStyleTest {
    @Test
    fun everyCharacterRangeALabelCanAskForHasAFontFile() {
        // The map engine never gets an answer for a range that has no file,
        // and the tile the label belongs to then stays unfinished: an
        // airspace named with a dash was not drawn at all.
        val fonts = TestSupport.assets.resolve(AppFiles.MAP_REGION).resolve("fonts/noto-sans-regular")
        for (start in 0 until 65536 step 256) {
            val file = fonts.resolve("$start-${start + 255}.pbf")
            assertTrue(file.path, file.isFile)
        }
        // Latin, the dashes and quotation marks, Greek and Cyrillic are real
        // glyphs, not placeholders.
        for (range in listOf("0-255", "256-511", "768-1023", "1024-1279", "8192-8447")) {
            assertTrue(range, fonts.resolve("$range.pbf").length() > 10_000)
        }
    }

    private val template = TestSupport.files.text(AppFiles.MAP_STYLE)
    private val region = TestSupport.assets.resolve(AppFiles.MAP_REGION)

    private fun style(source: MapStyle.Source, region: File = this.region): JsonObject {
        // With Venti set, as a pilot's phone has it: everything the shared
        // file has is in the style.
        val text = MapStyle.json(source, template, region, server = "https://venti.example", appDomain = "venti.example")
        assertFalse("a placeholder was not replaced", text.contains("__"))
        val style = Json.parseToJsonElement(text).jsonObject
        assertEquals(8, style.getValue("version").jsonPrimitive.int)
        return style
    }

    /**
     * The two sources each place has in its own way: the base map and the
     * terrain. The aerial view's photographs come from their providers
     * wherever the rest is drawn from.
     */
    private fun sources(style: JsonObject): Map<String, JsonElement> {
        val sources = style.getValue("sources").jsonObject
        // The photographs and the thermal map come from their providers wherever the rest is drawn from.
        assertEquals(setOf("basemap", "terrain"), sources.keys.filterNot { it.startsWith("aerial-") || it == "thermals" }.toSet())
        assertFalse(style.toString().contains(MapStyle.SERVER_PLACEHOLDER))
        return sources.filterKeys { !it.startsWith("aerial-") && it != "thermals" }
    }

    @Test
    fun everyTileSourceOfTheBundledStyleIsAFileShippedWithTheApp() {
        for ((name, source) in sources(style(MapStyle.Source.Bundled))) {
            val url = source.jsonObject.getValue("url").jsonPrimitive.content
            assertTrue(url, url.startsWith("pmtiles://file:///"))
            assertTrue("$name: $url", File(url.removePrefix("pmtiles://file://")).isFile)
        }
    }

    @Test
    fun theOnlineStyleFetchesTilesAndKeepsTheTerrainSettings() {
        val sources = sources(style(MapStyle.Source.Online))

        assertEquals(
            "https://tiles.openfreemap.org/planet",
            sources.getValue("basemap").jsonObject.getValue("url").jsonPrimitive.content,
        )
        val terrain = sources.getValue("terrain").jsonObject
        assertNull(terrain["url"])
        assertEquals(
            listOf("https://tiles.mapterhorn.com/{z}/{x}/{y}.webp"),
            terrain.getValue("tiles").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(12, terrain.getValue("maxzoom").jsonPrimitive.int)
        assertEquals("terrarium", terrain.getValue("encoding").jsonPrimitive.content)
        assertEquals(512, terrain.getValue("tileSize").jsonPrimitive.int)
        assertEquals("raster-dem", terrain.getValue("type").jsonPrimitive.content)
        assertTrue(terrain.getValue("attribution").jsonPrimitive.content.contains("Mapterhorn"))
    }

    @Test
    fun labelFontsAlwaysComeFromInsideTheApp() {
        for (source in MapStyle.Source.entries) {
            val glyphs = style(source).getValue("glyphs").jsonPrimitive.content
            assertTrue(glyphs, glyphs.startsWith("file:///"))
            assertTrue(glyphs, glyphs.endsWith("/fonts/{fontstack}/{range}.pbf"))
        }
        for (range in listOf("0-255", "256-511")) {
            assertTrue(range, region.resolve("fonts/noto-sans-regular/$range.pbf").isFile)
        }
    }

    @Test
    fun theStyleKeepsEveryLayerOfTheSharedFile() {
        val shared = Json.parseToJsonElement(template).jsonObject.getValue("layers")

        assertEquals(shared, style(MapStyle.Source.Online).getValue("layers"))
        assertEquals(shared, style(MapStyle.Source.Bundled).getValue("layers"))
    }

    /** A small piece of the map around a place, as the screen shows it at zoom 12. */
    private fun around(lat: Double, lon: Double) = Bounds(lat - 0.05, lon - 0.08, lat + 0.05, lon + 0.08)

    @Test
    fun theShortCreditNamesTheMakersOfThePhotographsInView() {
        val photos = MapStyle.aerialPhotos(MapStyle.json(MapStyle.Source.Online, template, region, server = "https://venti.example"))
        assertEquals(
            listOf(
                "EOX", "swisstopo", "Autonome Provinz Bozen - Südtirol", "IGN", "basemap.at", "LGL-BW",
                "Bayerische Vermessungsverwaltung",
            ),
            photos.map { it.credit },
        )
        // Over the Tegernsee: Austria's box takes in the south of Bavaria.
        assertEquals(
            "Photos: basemap.at · Bayerische Vermessungsverwaltung · EOX",
            MapStyle.photosCredit(photos, around(47.70, 11.76), 12.0),
        )
        assertEquals("Photos: swisstopo · EOX", MapStyle.photosCredit(photos, around(46.69, 7.86), 12.0))
        // France's box takes in the west of Switzerland, and its source has
        // tiles from zoom 13 only, though its layer begins at 12.
        assertEquals("Photos: swisstopo · IGN · EOX", MapStyle.photosCredit(photos, around(46.69, 7.86), 13.0))
        // Over the Black Forest: Baden-Württemberg's own, and France's
        // box, which reaches over the Rhine.
        assertEquals("Photos: IGN · LGL-BW · EOX", MapStyle.photosCredit(photos, around(47.90, 8.10), 13.0))
        assertEquals("Photos: LGL-BW · EOX", MapStyle.photosCredit(photos, around(47.90, 8.10), 12.0))
        // Zoomed out, only the world's mosaic is drawn, anywhere.
        assertEquals("Photos: EOX", MapStyle.photosCredit(photos, around(47.70, 11.76), 8.0))
        assertEquals("Photos: EOX", MapStyle.photosCredit(photos, around(46.69, 7.86), 8.0))
        // Each name once.
        assertEquals("Photos: EOX", MapStyle.photosCredit(photos + photos.first(), around(0.0, 0.0), 12.0))
        assertNull(MapStyle.photosCredit(emptyList(), around(47.70, 11.76), 12.0))
    }

    @Test
    fun theCreditsPageNamesEveryMakerOfPhotographsInTheStylesOwnWords() {
        // The shared file itself, as the credits page reads it: every layer
        // of photographs, also those that go through Venti.
        val photos = MapStyle.aerialPhotos(template)
        assertEquals(
            listOf(
                "aerial-world", "aerial-switzerland", "aerial-south-tyrol", "aerial-france", "aerial-austria",
                "aerial-baden-wuerttemberg", "aerial-bavaria",
            ),
            photos.map { it.layer },
        )
        val swiss = photos.single { it.layer == "aerial-switzerland" }
        assertEquals("©swisstopo. Includes material © (2016-2018) Planet. All rights reserved", swiss.wording)
        assertEquals(listOf("https://www.swisstopo.admin.ch"), swiss.links)
        val world = photos.single { it.layer == "aerial-world" }
        assertEquals(
            "EOxCloudless by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025), CC BY-NC-SA 4.0",
            world.wording,
        )
        assertEquals(
            listOf("https://cloudless.eox.at", "https://eox.at", "https://creativecommons.org/licenses/by-nc-sa/4.0/"),
            world.links,
        )
        for (maker in photos) {
            assertTrue(maker.layer, maker.wording.isNotBlank() && '<' !in maker.wording)
        }
        assertEquals("A & B", MapStyle.plainText(" <a href=\"x\">A</a> &amp; B "))
        assertEquals(listOf("x", "y"), MapStyle.links("<a href=\"x\">1</a> <a href=\"y\">2</a> <a href=\"x\">3</a>"))
    }

    @Test
    fun anAppWithoutAServerDoesNotNameThePhotographsItCannotShow() {
        // Without a server the style leaves out France's and Austria's photographs, which go through Venti.
        val photos = MapStyle.aerialPhotos(MapStyle.json(MapStyle.Source.Online, template, region, server = null))
        assertFalse(photos.any { it.credit == "basemap.at" || it.credit == "IGN" })
        assertEquals("Photos: swisstopo · EOX", MapStyle.photosCredit(photos, around(46.69, 7.86), 13.0))
        assertEquals(
            "Photos: Bayerische Vermessungsverwaltung · EOX",
            MapStyle.photosCredit(photos, around(47.70, 11.76), 12.0),
        )
    }

    @Test
    fun theAerialViewIsHiddenUntilItIsAskedFor() {
        val views = MapStyle.views(template)
        assertEquals(
            listOf(
                "aerial-world",
                "aerial-switzerland",
                "aerial-south-tyrol",
                "aerial-france",
                "aerial-austria",
                "aerial-baden-wuerttemberg",
                "aerial-bavaria",
                "aerial-road",
                "aerial-aerialway-casing",
            ),
            views.aerial,
        )
        assertEquals(listOf("path", "track", "road-minor", "road-major", "rail"), views.drawn)
        // Every aerial layer is hidden in every prepared style: nothing is
        // fetched from the photographs' providers until the view is on.
        for (source in MapStyle.Source.entries) {
            val layers = style(source).getValue("layers").jsonArray.map { it.jsonObject }
            for (name in views.aerial) {
                val layer = layers.single { it.getValue("id").jsonPrimitive.content == name }
                assertEquals(name, "none", layer["layout"]?.jsonObject?.get("visibility")?.jsonPrimitive?.content)
            }
            // The photographs are raster tiles, and each names whose they are.
            val sources = style(source).getValue("sources").jsonObject
            for (name in sources.keys.filter { it.startsWith("aerial-") }) {
                val photographs = sources.getValue(name).jsonObject
                assertEquals("raster", photographs.getValue("type").jsonPrimitive.content)
                assertTrue(name, photographs.getValue("attribution").jsonPrimitive.content.isNotBlank())
            }
        }
        assertThrows(MapStyle.StyleError::class.java) { MapStyle.views("{}") }
    }

    @Test
    fun franceAndAustriasPhotographsAreAskedOfVentiAndLeftOutWithoutIt() {
        val throughVenti = mapOf("aerial-france" to "ign-ortho", "aerial-austria" to "basemap-at")
        for (source in MapStyle.Source.entries) {
            val with = style(source)
            for ((name, path) in throughVenti) {
                assertEquals(
                    listOf("https://venti.example/v1/imagery/$path/{z}/{x}/{y}"),
                    with.getValue("sources").jsonObject.getValue(name).jsonObject.getValue("tiles").jsonArray
                        .map { it.jsonPrimitive.content },
                )
                assertTrue(name in layerNames(with))
            }

            val text = MapStyle.json(source, template, region, server = null, appDomain = "venti.example")
            assertFalse(text, text.contains("__"))
            val without = Json.parseToJsonElement(text).jsonObject
            for (name in throughVenti.keys) {
                assertFalse(name in without.getValue("sources").jsonObject)
                assertFalse(name in layerNames(without))
            }
            assertEquals(layerNames(with) - throughVenti.keys, layerNames(without))
        }
    }

    /** A style of its own that asks Venti for one source, drawn by one layer, beside the two every style has. */
    private val askingVenti = """
        {"version": 8, "sources": {
          "basemap": {"type": "vector", "url": "pmtiles://__BASEMAP__"},
          "terrain": {"type": "raster-dem", "url": "pmtiles://__TERRAIN__", "encoding": "terrarium", "tileSize": 512},
          "aerial-austria": {"type": "raster", "tiles": ["__VENTI_SERVER__/v1/imagery/basemap-at/{z}/{x}/{y}"], "tileSize": 256}
        }, "layers": [
          {"id": "land", "type": "background"},
          {"id": "aerial-austria", "type": "raster", "source": "aerial-austria", "metadata": {"venti:view": "aerial"}, "layout": {"visibility": "none"}},
          {"id": "water", "type": "fill", "source": "basemap", "source-layer": "water"}
        ]}
    """.trimIndent()

    private fun layerNames(style: JsonObject): List<String> =
        style.getValue("layers").jsonArray.map { it.jsonObject.getValue("id").jsonPrimitive.content }

    @Test
    fun whatTheStyleAsksOfVentiIsAskedOfTheServerSet() {
        val text = MapStyle.json(MapStyle.Source.Online, askingVenti, region, server = "https://venti.example")
        assertFalse(text, text.contains("__"))
        val style = Json.parseToJsonElement(text).jsonObject
        assertEquals(
            listOf("https://venti.example/v1/imagery/basemap-at/{z}/{x}/{y}"),
            style.getValue("sources").jsonObject.getValue("aerial-austria").jsonObject.getValue("tiles").jsonArray
                .map { it.jsonPrimitive.content },
        )
        assertEquals(listOf("land", "aerial-austria", "water"), layerNames(style))
    }

    @Test
    fun withoutVentiWhatItWouldBeAskedForIsLeftOut() {
        for (source in MapStyle.Source.entries) {
            val text = MapStyle.json(source, askingVenti, region, server = null)
            assertFalse(text, text.contains("__"))
            val style = Json.parseToJsonElement(text).jsonObject
            assertEquals(setOf("basemap", "terrain"), style.getValue("sources").jsonObject.keys)
            assertEquals(listOf("land", "water"), layerNames(style))
        }
    }

    @Test
    fun aStyleThatAsksNothingOfVentiComesOutAsBefore() {
        // The same style without the source that names Venti.
        val asksNothing = Json.parseToJsonElement(askingVenti).jsonObject.let { style ->
            JsonObject(
                style + mapOf(
                    "sources" to JsonObject(style.getValue("sources").jsonObject - "aerial-austria"),
                    "layers" to JsonArray(
                        style.getValue("layers").jsonArray.filterNot {
                            it.jsonObject.getValue("id").jsonPrimitive.content == "aerial-austria"
                        },
                    ),
                ),
            ).toString()
        }
        val without = MapStyle.json(MapStyle.Source.Online, asksNothing, region, server = null)
        assertEquals(without, MapStyle.json(MapStyle.Source.Online, asksNothing, region, server = "https://venti.example"))
        assertEquals(listOf("land", "water"), layerNames(Json.parseToJsonElement(without).jsonObject))
    }

    /** A style of its own whose one source asks for the app's domain, and another that asks for both. */
    private val askingTheDomain = """
        {"version": 8, "sources": {
          "basemap": {"type": "vector", "url": "pmtiles://__BASEMAP__"},
          "terrain": {"type": "raster-dem", "url": "pmtiles://__TERRAIN__", "encoding": "terrarium", "tileSize": 512},
          "thermals": {"type": "raster", "scheme": "tms", "tiles": ["https://thermals.example/{z}/{x}/{y}.png?src=__APP_DOMAIN__"]},
          "both": {"type": "raster", "tiles": ["__VENTI_SERVER__/tiles/{z}/{x}/{y}?src=__APP_DOMAIN__"]}
        }, "layers": [
          {"id": "land", "type": "background"},
          {"id": "thermals", "type": "raster", "source": "thermals", "metadata": {"venti:layer": "thermals"}, "layout": {"visibility": "none"}},
          {"id": "both", "type": "raster", "source": "both"},
          {"id": "water", "type": "fill", "source": "basemap", "source-layer": "water"}
        ]}
    """.trimIndent()

    @Test
    fun aPlaceholderWithoutAValueLeavesItsSourceAndItsLayersOut() {
        // No domain: nothing that asks for one, whatever else it has.
        val none = MapStyle.json(MapStyle.Source.Online, askingTheDomain, region, server = "https://venti.example", appDomain = null)
        assertFalse(none, none.contains("__"))
        val without = Json.parseToJsonElement(none).jsonObject
        assertEquals(setOf("basemap", "terrain"), without.getValue("sources").jsonObject.keys)
        assertEquals(listOf("land", "water"), layerNames(without))

        // A domain and no server: the source that asks only for the domain.
        val domain = MapStyle.json(MapStyle.Source.Online, askingTheDomain, region, server = null, appDomain = "venti.example")
        assertFalse(domain, domain.contains("__"))
        val withDomain = Json.parseToJsonElement(domain).jsonObject
        assertEquals(
            listOf("https://thermals.example/{z}/{x}/{y}.png?src=venti.example"),
            withDomain.getValue("sources").jsonObject.getValue("thermals").jsonObject.getValue("tiles").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(listOf("land", "thermals", "water"), layerNames(withDomain))

        // Both: every placeholder of an address is filled in.
        val both = Json.parseToJsonElement(
            MapStyle.json(MapStyle.Source.Online, askingTheDomain, region, server = "https://venti.example", appDomain = "venti.example"),
        ).jsonObject
        assertEquals(
            listOf("https://venti.example/tiles/{z}/{x}/{y}?src=venti.example"),
            both.getValue("sources").jsonObject.getValue("both").jsonObject.getValue("tiles").jsonArray.map { it.jsonPrimitive.content },
        )
        assertEquals(listOf("land", "thermals", "both", "water"), layerNames(both))
    }

    @Test
    fun theThermalMapIsOfferedOnlyWhereTheStyleHasItsLayers() {
        assertEquals(listOf("thermals"), MapStyle.views(template).thermals)
        val withDomain = MapStyle.json(MapStyle.Source.Online, template, region, server = null, appDomain = "venti.example")
        assertEquals(listOf("thermals"), MapStyle.views(withDomain).thermals)
        val thermals = Json.parseToJsonElement(withDomain).jsonObject.getValue("sources").jsonObject.getValue("thermals").jsonObject
        assertTrue(thermals.toString(), thermals.getValue("tiles").jsonArray.single().jsonPrimitive.content.endsWith("?src=venti.example"))
        // Hidden until the pilot switches it on.
        val layer = Json.parseToJsonElement(withDomain).jsonObject.getValue("layers").jsonArray.map { it.jsonObject }
            .single { it.getValue("id").jsonPrimitive.content == "thermals" }
        assertEquals("none", layer["layout"]?.jsonObject?.get("visibility")?.jsonPrimitive?.content)
        // Without the app's domain there is nothing to offer, and no chip.
        val withoutDomain = MapStyle.json(MapStyle.Source.Online, template, region, server = null, appDomain = null)
        assertTrue(MapStyle.views(withoutDomain).thermals.isEmpty())
        // The app's own domain, the owner's, is named to the author's server.
        val withTheApps = MapStyle.json(MapStyle.Source.Online, template, region)
        assertEquals(listOf("thermals"), MapStyle.views(withTheApps).thermals)
        val asked = Json.parseToJsonElement(withTheApps).jsonObject.getValue("sources").jsonObject.getValue("thermals").jsonObject
        assertTrue(asked.getValue("tiles").jsonArray.single().jsonPrimitive.content.endsWith("?src=venti.fly"))
    }

    @Test
    fun aRegionKeptForFlyingWithoutANetworkKeepsTheAerialViewHiddenAndItsOwnGroundUnderIt() {
        val kept = fly.venti.app.maps.KeptMap(
            id = "alps",
            version = "1",
            basemap = region.resolve("basemap.pmtiles"),
            terrain = region.resolve(MapStyle.BUNDLED_TERRAIN),
        )
        val text = MapStyle.json(MapStyle.Source.Online, template, region, kept, server = "https://venti.example")
        val style = Json.parseToJsonElement(text).jsonObject
        val layers = style.getValue("layers").jsonArray.map { it.jsonObject }
        // The cockpit's map over a kept region can show the photographs
        // when the pilot asks for them; until then nothing is fetched.
        val views = MapStyle.views(text)
        assertEquals(MapStyle.views(template).aerial, views.aerial)
        for (name in views.aerial) {
            val layer = layers.single { it.getValue("id").jsonPrimitive.content == name }
            assertEquals(name, "none", layer["layout"]?.jsonObject?.get("visibility")?.jsonPrimitive?.content)
        }
        // Under the photographs the kept region's own ground, relief and
        // names stay: where photographs do not load, that is what shows.
        val names = layerNames(style)
        for (ground in listOf("land", "hillshade", "water", "place")) {
            assertTrue(ground, ground in names)
            assertTrue(ground, names.indexOf(ground) < names.indexOf("aerial-world") || ground == "place")
        }
        assertTrue(names.indexOf("place") > names.indexOf("aerial-world"))
    }

    @Test
    fun aRegionKeptForFlyingWithoutANetworkNeverAsksForTheThermalMap() {
        val kept = fly.venti.app.maps.KeptMap(
            id = "alps",
            version = "1",
            basemap = region.resolve("basemap.pmtiles"),
            terrain = region.resolve(MapStyle.BUNDLED_TERRAIN),
        )
        val text = MapStyle.json(MapStyle.Source.Online, template, region, kept, server = "https://venti.example", appDomain = "venti.example")
        val style = Json.parseToJsonElement(text).jsonObject
        assertFalse("thermals" in style.getValue("sources").jsonObject)
        assertTrue(MapStyle.views(text).thermals.isEmpty())
    }

    @Test
    fun theThermalMapsChoiceIsKept() {
        val preferences = fly.venti.app.settings.MemoryPreferences()
        assertFalse(preferences.mapShowsThermals)
        preferences.mapShowsThermals = true
        assertTrue(preferences.mapShowsThermals)
        // The cockpit's is its own, and on until switched off.
        assertTrue(preferences.cockpitShowsThermals)
        preferences.cockpitShowsThermals = false
        assertFalse(preferences.cockpitShowsThermals)
        assertTrue(preferences.mapShowsThermals)
    }

    @Test
    fun theAppsDomainIsTheOwners() {
        assertEquals("venti.fly", MapStyle.APP_DOMAIN)
    }

    @Test
    fun aStyleWithoutItsSourcesIsRefused() {
        assertThrows(MapStyle.StyleError::class.java) { MapStyle.json(MapStyle.Source.Online, "{}", region) }
        assertThrows(MapStyle.StyleError::class.java) { MapStyle.json(MapStyle.Source.Online, "not json", region) }
    }

    @Test
    fun theShippedMapFilesAreCopiedOutOnceAndAgainForANewVersion() {
        val folder = TestSupport.newFolder("map").resolve("map")

        val installed = MapRegion.install(TestSupport.files, folder, stamp = "1")

        for (file in listOf("basemap.pmtiles", "terrain.pmtiles", "fonts/noto-sans-regular/0-255.pbf")) {
            assertEquals(file, region.resolve(file).length(), installed.resolve(file).length())
        }
        // The style prepared for the copy points into it.
        val glyphs = style(MapStyle.Source.Bundled, installed).getValue("glyphs").jsonPrimitive.content
        assertTrue(glyphs, glyphs.startsWith("file://${installed.absolutePath}/fonts/"))

        // The same version leaves the files alone; a new one copies again.
        val terrain = installed.resolve("terrain.pmtiles")
        terrain.writeText("changed")
        MapRegion.install(TestSupport.files, folder, stamp = "1")
        assertEquals("changed", terrain.readText())
        MapRegion.install(TestSupport.files, folder, stamp = "2")
        assertEquals(region.resolve("terrain.pmtiles").length(), terrain.length())
    }
}
