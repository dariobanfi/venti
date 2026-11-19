package fly.venti.app.wind

import fly.venti.app.TestSupport
import fly.venti.core.WindBand
import fly.venti.core.WindStation
import fly.venti.core.WindSymbol
import fly.venti.core.serverStatus
import fly.venti.core.windMap
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.maplibre.geojson.Point
import java.util.Locale

/**
 * How a station's mark is drawn and read. The mark itself is the core's:
 * the stations here are the ones the real server recorded for its own
 * tests, read by the real core.
 */
class WindMarkTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    private fun recorded(name: String) = TestSupport.recordedAnswers.resolve(name).readBytes()

    /** The server's clock when the answers were recorded. */
    private val recordedAt: Long = serverStatus(200.toUShort(), recorded("health.json")).timeUnixS

    /** The recorded stations as they look so many minutes after they were recorded. */
    private fun stations(minutesLater: Long = 0): List<WindStation> =
        windMap(200.toUShort(), recorded("stations.json"), recordedAt + minutesLater * 60).stations

    private fun station(name: String, minutesLater: Long = 0): WindStation =
        stations(minutesLater).first { it.name.startsWith(name) }

    @Test
    fun everyStationComesMarkedByTheCore() {
        // 14.9 m/s from 260 degrees, gusting 25.7: an arrow pointing where the wind blows.
        val summit = station("Zugspitze").mark
        assertEquals(WindSymbol.Arrow(WindBand.STRONG), summit.symbol)
        assertEquals(80.0, summit.headingDeg, 1e-9)
        assertEquals("54/93", summit.label)

        // 0.2 m/s: too weak for a direction.
        val calm = station("Innsbruck").mark
        assertEquals(WindSymbol.Calm, calm.symbol)
        assertEquals("1/5", calm.label)

        // Two hours later nothing says what the wind does now.
        for (old in stations(minutesLater = 120)) {
            assertEquals(WindSymbol.Old, old.mark.symbol)
            assertEquals("", old.mark.label)
        }
    }

    @Test
    fun strongerWindIsDarkerAndDrawnLarger() {
        val scales = WindBand.entries.map { it.arrowScale }
        assertEquals(scales.sorted(), scales)
        assertEquals(WindBand.entries.size, scales.toSet().size)
        assertEquals(WindBand.entries.size, WindBand.entries.map { it.rgb }.toSet().size)
        // Darker means less light in every band than in the one before.
        val light = WindBand.entries.map { band ->
            (band.rgb shr 16 and 0xFF) + (band.rgb shr 8 and 0xFF) + (band.rgb and 0xFF)
        }
        assertEquals(light.sortedDescending(), light)

        assertEquals(1.3, WindSymbol.Arrow(WindBand.STRONG).scale, 1e-9)
        assertEquals(1.0, WindSymbol.Calm.scale, 1e-9)
        assertEquals(1.0, WindSymbol.Old.scale, 1e-9)
    }

    @Test
    fun everySymbolHasAPictureOfItsOwn() {
        val symbols = WindBand.entries.map { WindSymbol.Arrow(it) } + WindSymbol.Calm + WindSymbol.Old
        val pictures = symbols.map { it.picture }
        assertEquals(symbols.size, pictures.toSet().size)
        assertEquals("wind-arrow-strong", WindSymbol.Arrow(WindBand.STRONG).picture)
        assertEquals("wind-calm", WindSymbol.Calm.picture)
        assertEquals("wind-old", WindSymbol.Old.picture)
    }

    @Test
    fun whereStationsCrowdTheWindiestComeFirst() {
        val byRank = stations().sortedBy(WindFeature::rank).map { it.name }
        assertEquals("Zugspitze", byRank.first())
        assertEquals("Hohenpeißenberg", byRank[1])
        // Stations with nothing current to say come last.
        for (old in stations(minutesLater = 120)) {
            assertEquals(1000.0, WindFeature.rank(old), 1e-9)
        }
    }

    @Test
    fun theMapIsToldWhatToDrawForEachStation() {
        val summit = station("Zugspitze")
        val feature = WindFeature.of(summit)
        val place = feature.geometry() as Point
        assertEquals(summit.position.lat, place.latitude(), 1e-9)
        assertEquals(summit.position.lon, place.longitude(), 1e-9)
        assertEquals(summit.id, feature.getStringProperty(WindFeature.ID))
        assertEquals("wind-arrow-strong", feature.getStringProperty(WindFeature.PICTURE))
        assertEquals(80.0, feature.getNumberProperty(WindFeature.HEADING).toDouble(), 1e-9)
        assertEquals(1.3, feature.getNumberProperty(WindFeature.SCALE).toDouble(), 1e-9)
        assertEquals("54/93", feature.getStringProperty(WindFeature.LABEL))
        assertEquals(-25.7, feature.getNumberProperty(WindFeature.RANK).toDouble(), 1e-9)
    }

    @Test
    fun aRowSaysTheHeightAndTheAgeOnlyWhenItMatters() {
        val summit = station("Zugspitze")
        assertEquals("2,956 m", StationWords.detail(summit, recordedAt).substringBefore(" ·"))
        assertEquals(
            "Wind 54 kilometres per hour, gusts 93, from W",
            StationWords.spoken(summit, summit.mark, StationWords.detail(summit, recordedAt)),
        )

        val later = recordedAt + 120 * 60
        val old = station("Zugspitze", minutesLater = 120)
        val detail = StationWords.detail(old, later)
        assertTrue(detail, detail.endsWith("3 hours ago"))
        assertTrue(StationWords.spoken(old, old.mark, detail).startsWith("No current reading."))

        // Too weak for a direction: none is spoken. And one is one.
        val calm = station("Innsbruck")
        assertEquals(
            "Wind 1 kilometre per hour, gusts 5",
            StationWords.spoken(calm, calm.mark, StationWords.detail(calm, recordedAt)),
        )
    }
}
