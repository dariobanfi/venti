package fly.venti.app.wind

import fly.venti.app.TestSupport
import fly.venti.core.WindStation
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
 * How a station is drawn and read, from the stations the real server
 * recorded for its own tests, read by the real core.
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
    fun strengthIsFourBandsOfTheMeanWind() {
        fun band(kilometresPerHour: Double) = WindBand.of(kilometresPerHour / 3.6)
        assertEquals(WindBand.Light, band(0.0))
        assertEquals(WindBand.Light, band(9.9))
        assertEquals(WindBand.Moderate, band(10.1))
        assertEquals(WindBand.Moderate, band(19.9))
        assertEquals(WindBand.Fresh, band(20.1))
        assertEquals(WindBand.Fresh, band(29.9))
        assertEquals(WindBand.Strong, band(30.1))
        assertEquals(WindBand.Strong, band(120.0))
        // Stronger wind is darker and drawn larger.
        val scales = WindBand.entries.map { it.arrowScale }
        assertEquals(scales.sorted(), scales)
        assertEquals(4, WindBand.entries.map { it.rgb }.toSet().size)
    }

    @Test
    fun anArrowPointsWhereTheWindBlowsAndSaysMeanAndGusts() {
        // 14.9 m/s from 260 degrees, gusting 25.7.
        val summit = station("Zugspitze")
        val mark = WindMark.of(summit)
        assertEquals(WindMark.Symbol.Arrow(WindBand.Strong), mark.symbol)
        assertEquals(80.0, mark.heading, 1e-9)
        assertEquals("54/93", mark.label)
        assertEquals("wind-arrow-3", mark.picture)
        assertEquals(WindBand.Strong.arrowScale, mark.scale, 1e-9)

        // A northerly: the arrow points south.
        val valley = WindMark.of(station("Garmisch"))
        assertEquals(WindMark.Symbol.Arrow(WindBand.Light), valley.symbol)
        assertEquals(170.0, valley.heading, 1e-9)
        assertEquals("5/8", valley.label)
    }

    @Test
    fun gustsAreLeftOutWhenUnknownOrNoDifferentFromTheMean() {
        assertEquals("11", WindMark.speeds(3.0, null))
        assertEquals("11", WindMark.speeds(3.0, 3.1))
        assertEquals("11/14", WindMark.speeds(3.0, 4.0))
        assertEquals("0", WindMark.speeds(0.0, 0.1))
    }

    @Test
    fun windTooWeakForADirectionIsADotWithItsNumbers() {
        // 0.2 m/s, gusting 1.4.
        val mark = WindMark.of(station("Innsbruck"))
        assertEquals(WindMark.Symbol.Calm, mark.symbol)
        assertEquals("1/5", mark.label)
        assertEquals("wind-calm", mark.picture)
        assertEquals(1.0, mark.scale, 1e-9)
    }

    @Test
    fun aReadingTooOldIsAGreyDotWithoutNumbers() {
        // Two hours after it was recorded nothing says what the wind does now.
        for (old in stations(minutesLater = 120)) {
            assertTrue(old.isOld)
            val mark = WindMark.of(old)
            assertEquals(WindMark.Symbol.Old, mark.symbol)
            assertEquals("", mark.label)
            assertEquals("wind-old", mark.picture)
            assertEquals(1000.0, WindMark.rank(old), 1e-9)
        }
    }

    @Test
    fun whereStationsCrowdTheWindiestComeFirst() {
        val byRank = stations().sortedBy(WindMark::rank).map { it.name }
        assertEquals("Zugspitze", byRank.first())
        assertEquals("Hohenpeißenberg", byRank[1])
        // Gusts decide, not the mean: a calm station that gusts keeps its place.
        assertTrue(WindMark.rank(station("Zugspitze")) < WindMark.rank(station("Hohenpeißenberg")))
    }

    @Test
    fun theMapIsToldWhatToDrawForEachStation() {
        val summit = station("Zugspitze")
        val feature = WindFeature.of(summit)
        val place = feature.geometry() as Point
        assertEquals(summit.position.lat, place.latitude(), 1e-9)
        assertEquals(summit.position.lon, place.longitude(), 1e-9)
        assertEquals(summit.id, feature.getStringProperty(WindFeature.ID))
        assertEquals("wind-arrow-3", feature.getStringProperty(WindFeature.PICTURE))
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
            StationWords.spoken(summit, WindMark.of(summit), StationWords.detail(summit, recordedAt)),
        )

        val later = recordedAt + 120 * 60
        val old = station("Zugspitze", minutesLater = 120)
        val detail = StationWords.detail(old, later)
        assertTrue(detail, detail.endsWith("3 hours ago"))
        assertTrue(StationWords.spoken(old, WindMark.of(old), detail).startsWith("No current reading."))

        // Too weak for a direction: none is spoken.
        val calm = station("Innsbruck")
        assertEquals(
            "Wind 1 kilometres per hour, gusts 5",
            StationWords.spoken(calm, WindMark.of(calm), StationWords.detail(calm, recordedAt)),
        )
    }
}
