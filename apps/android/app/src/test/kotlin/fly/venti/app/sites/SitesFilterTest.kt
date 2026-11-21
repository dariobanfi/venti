package fly.venti.app.sites

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.Bounds
import fly.venti.core.Coordinate
import fly.venti.core.SiteKind
import fly.venti.core.SitesFilter
import fly.venti.core.SitesShown
import fly.venti.core.SitesWindIs
import fly.venti.core.serverStatus
import fly.venti.core.windMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.maplibre.geojson.Polygon

/**
 * The filter of the sites between the core and the screens, against the
 * real core and the answer the server's own tests keep for `/v1/sites`:
 * three take-offs east of the Tegernsee, none with wind directions and one
 * with a height, and two landings; one take-off and one landing are mapped
 * as areas. Which sites fit and every word about it are the core's and are
 * tested there; here it is the plumbing.
 */
class SitesFilterTest {
    private fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

    /** The model with the recorded valley's five sites. */
    private fun model(): SitesModel = runBlocking {
        val still = CoroutineScope(Job().apply { cancel() })
        SitesModel(still, transport = { HttpAnswer(200, recorded("sites.json")) }).also { sites ->
            sites.server = { BASE }
            sites.load(Bounds(south = 47.6, west = 11.7, north = 47.75, east = 11.95), BASE)
            assertEquals(5, sites.sites.size)
        }
    }

    @Test
    fun aFilterIsKeptAsThePilotLeftItAndReadBack() {
        val asked = SitesFilter(
            shown = SitesShown.TAKEOFFS,
            windFromDeg = 292.5,
            windIs = SitesWindIs.CHOSEN,
            takeoffsFromM = 1500.0,
        )
        assertEquals(asked, SitesFilterWords.read(SitesFilterWords.kept(asked)))
        val now = SitesFilterWords.NOTHING.copy(windFromDeg = 184.0, windIs = SitesWindIs.MEASURED_NOW)
        assertEquals(now, SitesFilterWords.read(SitesFilterWords.kept(now)))
        assertEquals(SitesFilterWords.NOTHING, SitesFilterWords.read(SitesFilterWords.kept(SitesFilterWords.NOTHING)))
        // Nothing kept, and what is no filter: every site is shown.
        for (none in listOf(null, "", "TAKEOFFS", "SOMETHING|1|CHOSEN|", "ALL||LATER|")) {
            assertEquals(none, SitesFilterWords.NOTHING, SitesFilterWords.read(none))
        }
        // Through the settings, as the map keeps it.
        val preferences = MemoryPreferences()
        assertNull(preferences.mapSitesFilter)
        preferences.mapSitesFilter = SitesFilterWords.kept(asked)
        assertEquals(asked, SitesFilterWords.read(preferences.mapSitesFilter))
        // The places from pilots are part of the filter: off until the pilot
        // switches them on, kept as they were left, and off in a line kept
        // before the switch was part of it.
        assertFalse(SitesFilterWords.read(preferences.mapSitesFilter).withPilots)
        val withPilots = asked.copy(withPilots = true)
        preferences.mapSitesFilter = SitesFilterWords.kept(withPilots)
        assertEquals(withPilots, SitesFilterWords.read(preferences.mapSitesFilter))
        assertEquals(SitesFilterWords.NOTHING, SitesFilterWords.read("ALL||CHOSEN|"))
        assertTrue(SitesFilterWords.read("ALL||CHOSEN||with-pilots").withPilots)
    }

    @Test
    fun theModelHandsOutWhatTheFilterLeavesInTheCoresWords() {
        val sites = model()
        // Nothing asked: every site, none apart.
        assertEquals(5, sites.filtered.fitting.size)
        assertTrue(sites.filtered.unsaid.isEmpty())
        assertFalse(sites.filtered.isOn)
        assertEquals("All sites", sites.filtered.filterWords)

        // A wind: none of the three take-offs names a direction, so all
        // three are shown apart, and the landings stay.
        sites.filter = SitesFilterWords.NOTHING.copy(windFromDeg = 315.0)
        val wind = sites.filtered
        assertTrue(wind.isOn)
        assertEquals(listOf(SiteKind.LANDING, SiteKind.LANDING), wind.fitting.map { it.kind })
        assertEquals(3, wind.unsaid.size)
        assertEquals("All sites, take-offs for a wind from NW", wind.filterWords)
        assertEquals("0 of 3 take-offs fit a wind from NW", wind.words)
        assertEquals("3 take-offs give no wind directions", wind.unsaidTitle)
        assertTrue(wind.unsaidNote, wind.unsaidNote.contains("shown apart, not left out"))

        // A height, of take-offs alone: one says it lies high enough, two do not say.
        sites.filter = SitesFilter(SitesShown.TAKEOFFS, null, SitesWindIs.CHOSEN, 1500.0)
        val height = sites.filtered
        assertEquals(listOf("Fluggelände Jägerkamp/Nagelspitz"), height.fitting.map { it.name })
        assertEquals("1 of 3 take-offs lies at 1500 m or higher", height.words)
        assertEquals("2 take-offs give no height", height.unsaidTitle)

        // Landings: the wind and the height are not asked.
        sites.filter = SitesFilter(SitesShown.LANDINGS, 315.0, SitesWindIs.CHOSEN, 1500.0)
        assertEquals("2 landings", sites.filtered.words)
        assertTrue(sites.filtered.unsaid.isEmpty())
    }

    @Test
    fun measuredNowTakesTheDirectionTheScreenReadsAndTheForecastsIsNotOffered() {
        val sites = model()
        sites.filter = SitesFilterWords.NOTHING.copy(windFromDeg = 90.0, windIs = SitesWindIs.MEASURED_NOW)
        // No station to read: the filter asks about no wind, whatever direction it was chosen with.
        assertNull(sites.asked.windFromDeg)
        assertFalse(sites.filtered.isOn)
        // The station's direction now, not the one of the moment it was chosen.
        sites.measuredNowDeg = 184.0
        assertEquals(184.0, sites.asked.windFromDeg)
        assertEquals("0 of 3 take-offs fit the S wind measured now", sites.filtered.words)
        assertEquals("All sites, take-offs for the S wind measured now", sites.filtered.filterWords)
        // A filter kept with the forecast's wind, which no screen here offers, asks about no wind.
        sites.filter = SitesFilterWords.NOTHING.copy(windFromDeg = 90.0, windIs = SitesWindIs.FORECAST_NOW)
        assertNull(sites.asked.windFromDeg)
        assertFalse(sites.filtered.isOn)
    }

    @Test
    fun measuredNowIsReadFromTheNearestStationThatIsNotOld() {
        // The stations the server's own tests keep, at the moment they were recorded.
        val now = serverStatus(200.toUShort(), recorded("health.json")).timeUnixS
        val stations = windMap(200.toUShort(), recorded("stations.json"), now).stations
        // Beside the station in Garmisch, which had a light wind from the north.
        val garmisch = stations.first { it.name == "Garmisch-Partenkirchen" }
        val beside = Coordinate(lat = garmisch.position.lat + 0.01, lon = garmisch.position.lon)
        assertEquals(garmisch.latest?.directionDeg, SitesFilterWords.measuredNow(stations, beside))
        assertNotNull(SitesFilterWords.measuredNow(stations, beside))
        // Far from every station: none to read.
        assertNull(SitesFilterWords.measuredNow(stations, Coordinate(lat = 40.0, lon = 0.0)))
        // Two hours on the readings no longer stand for the wind now.
        val old = windMap(200.toUShort(), recorded("stations.json"), now + 2 * 3600).stations
        assertNull(SitesFilterWords.measuredNow(old, beside))
        assertNull(SitesFilterWords.measuredNow(emptyList(), beside))
    }

    @Test
    fun theMapDrawsTheSitesShownApartDimmedAndUnderTheOthers() {
        val sites = model()
        sites.filter = SitesFilterWords.NOTHING.copy(windFromDeg = 315.0)
        val left = sites.filtered
        val signs = SiteFeature.all(left.fitting, left.unsaid)
        assertEquals(5, signs.size)
        // Those shown apart first, each with the dimmed picture of its kind.
        assertEquals(listOf(true, true, true, false, false), signs.map { it.getBooleanProperty(SiteFeature.DIMMED) })
        assertEquals(
            listOf("site-takeoff-unsaid", "site-takeoff-unsaid", "site-takeoff-unsaid", "site-landing", "site-landing"),
            signs.map { it.getStringProperty(SiteFeature.PICTURE) },
        )
        // A dimmed picture for every kind, none of them a kind's own.
        val pictures = SiteKind.entries.map { it.picture } + SiteKind.entries.map { it.dimmedPicture }
        assertEquals(SiteKind.entries.size * 2, pictures.toSet().size)
    }

    @Test
    fun aSiteMappedAsAnAreaHasItsOutlineUnderItsSign() {
        val sites = model()
        fun site(way: String) = sites.sites.first { it.id.endsWith(way) }
        // The landing field below the Jägerkamp, eight points, the last the first again.
        val field = checkNotNull(SiteAreas.of(site("way/1080988126")))
        val ring = (field.geometry() as Polygon).coordinates().single()
        assertEquals(8, ring.size)
        assertEquals(ring.first(), ring.last())
        assertFalse(field.getBooleanProperty(SiteFeature.DIMMED))
        // The take-off above it, five points, dimmed when it is shown apart.
        val takeoff = checkNotNull(SiteAreas.of(site("way/1080986173"), dimmed = true))
        assertEquals(5, (takeoff.geometry() as Polygon).coordinates().single().size)
        assertTrue(takeoff.getBooleanProperty(SiteFeature.DIMMED))
        // A site mapped as a point has none.
        assertNull(SiteAreas.of(site("node/9908030941")))

        // Under a filter: the outlines of what is left, those shown apart first.
        sites.filter = SitesFilterWords.NOTHING.copy(windFromDeg = 315.0)
        val areas = SiteAreas.all(sites.filtered.fitting, sites.filtered.unsaid)
        assertEquals(listOf(true, false), areas.map { it.getBooleanProperty(SiteFeature.DIMMED) })
        sites.filter = SitesFilterWords.NOTHING.copy(shown = SitesShown.LANDINGS)
        assertEquals(1, SiteAreas.all(sites.filtered.fitting, sites.filtered.unsaid).size)
    }

    private companion object {
        const val BASE = "https://wind.example.org"
    }
}
