package fly.venti.app.map

import fly.venti.app.AppFiles
import fly.venti.app.TestSupport
import kotlinx.serialization.json.Json
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
        val text = MapStyle.json(source, template, region)
        assertFalse("a placeholder was not replaced", text.contains("__"))
        val style = Json.parseToJsonElement(text).jsonObject
        assertEquals(8, style.getValue("version").jsonPrimitive.int)
        return style
    }

    private fun sources(style: JsonObject): JsonObject {
        val sources = style.getValue("sources").jsonObject
        assertEquals(setOf("basemap", "terrain"), sources.keys)
        return sources
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
