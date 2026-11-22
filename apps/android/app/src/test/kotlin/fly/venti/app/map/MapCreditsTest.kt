package fly.venti.app.map

import org.junit.Assert.assertEquals
import org.junit.Test

/** The base map's credits as the map's (i) lists them: the style's words without its links. */
class MapCreditsTest {
    @Test
    fun aCreditWithLinksIsWrittenAsWordsAlone() {
        assertEquals("© Mapterhorn", MapCredits.plain(MapStyle.Online.TERRAIN_ATTRIBUTION))
        assertEquals(
            "© OpenMapTiles © OpenStreetMap contributors",
            MapCredits.plain(
                "<a href=\"https://openmaptiles.org/\">&copy; OpenMapTiles</a> " +
                    "<a href=\"https://www.openstreetmap.org/copyright\">&copy; OpenStreetMap contributors</a>",
            ),
        )
        assertEquals("", MapCredits.plain("  "))
    }
}
