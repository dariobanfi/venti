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
        val calm = station("Innsbruck/Universitaet").mark
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
        assertEquals("2956 m", StationWords.detail(summit, recordedAt).substringBefore(" ·"))
        assertEquals(
            "Wind 54 kilometres per hour, gusts 93, from W, DWD",
            StationWords.spoken(summit, summit.mark, StationWords.detail(summit, recordedAt)),
        )

        val later = recordedAt + 120 * 60
        val old = station("Zugspitze", minutesLater = 120)
        val detail = StationWords.detail(old, later)
        assertEquals("2956 m · 3 hours ago · DWD", detail)
        assertTrue(StationWords.spoken(old, old.mark, detail).startsWith("No current reading."))

        // Too weak for a direction: none is spoken. And one is one.
        val calm = station("Innsbruck/Universitaet")
        assertEquals(
            "Wind 1 kilometre per hour, gusts 5, GeoSphere",
            StationWords.spoken(calm, calm.mark, StationWords.detail(calm, recordedAt)),
        )
    }

    @Test
    fun againstAnUnknownClockAReadingSaysItsOwnTime() {
        // Read with Venti's clock unknown (0): the core gives no age, and the reading's time stands where it stood.
        val unknown = windMap(200.toUShort(), recorded("stations.json"), 0).stations.first { it.name.startsWith("Zugspitze") }
        val reading = checkNotNull(unknown.latest)
        assertEquals(null, unknown.ageMin)
        assertEquals(fly.venti.app.design.Format.time(reading.timeUnixS), StationWords.age(unknown, reading, recordedAt + 3600))
        // With the clock known, the age.
        val known = station("Zugspitze", minutesLater = 25)
        assertEquals("70 minutes ago", StationWords.age(known, checkNotNull(known.latest), recordedAt + 25 * 60))
    }

    @Test
    fun aRowEndsWithItsNetworkAsTheCoresLineDoes() {
        val now = java.time.Instant.parse("2026-11-18T19:05:00Z").epochSecond
        val stations = windMap(200.toUShort(), MadeUp.networks, now).stations
        // At the answer's own time the row says what the core's line says.
        assertEquals(
            listOf("2691 m · MeteoSwiss", "2536 m · 3 hours ago · SLF", "no reading · Meteotrentino", "900 m · no reading"),
            stations.map { StationWords.detail(it, now) },
        )
        assertEquals(stations.map { it.line }, stations.map { StationWords.detail(it, now) })
        // Later by the phone's running clock the age counts on, and the network stays last.
        assertEquals("2536 m · 4 hours ago · SLF", StationWords.detail(stations[1], now + 3600))
    }

    /** A made-up answer: two stations of one name in two networks, and two networks late. */
    object MadeUp {
        val networks: ByteArray = """
            {
              "stations": [
                {"id": "meteoswiss-smn:WFJ", "source": "meteoswiss-smn", "name": "Weissfluhjoch",
                 "network": "MeteoSwiss", "position": {"lat": 46.833, "lon": 9.806},
                 "elevation_m": 2690.6,
                 "latest": {"time": "2026-11-18T19:00:00Z", "wind_speed_ms": 4.0}},
                {"id": "slf-imis:WFJ2", "source": "slf-imis", "name": "Weissfluhjoch",
                 "network": "SLF", "position": {"lat": 46.829, "lon": 9.809},
                 "elevation_m": 2536.0,
                 "latest": {"time": "2026-11-18T16:05:00Z", "wind_speed_ms": 4.0}},
                {"id": "meteotrentino:T0099", "source": "meteotrentino", "name": "Paganella",
                 "network": "Meteotrentino", "position": {"lat": 46.14, "lon": 11.04}},
                {"id": "old-server:1", "source": "old-server", "name": "Somewhere",
                 "position": {"lat": 46.0, "lon": 11.0}, "elevation_m": 900.0}
              ],
              "truncated": false,
              "attributions": [
                {"source": "meteotrentino", "provider": "Provincia Autonoma di Trento - Meteotrentino",
                 "short_name": "Meteotrentino", "licence": "CC BY 4.0", "text": "Meteotrentino",
                 "url": "https://www.meteotrentino.it/"},
                {"source": "slf-imis", "provider": "WSL Institute for Snow and Avalanche Research SLF",
                 "short_name": "SLF", "licence": "CC BY 4.0", "text": "SLF", "url": "https://www.slf.ch/"}
              ],
              "late_sources": ["meteotrentino", "slf-imis"]
            }
        """.trimIndent().toByteArray()
    }
}
