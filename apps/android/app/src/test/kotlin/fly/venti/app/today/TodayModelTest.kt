package fly.venti.app.today

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.sites.KeptSitesModel
import fly.venti.core.Coordinate
import fly.venti.core.SiteKind
import fly.venti.core.SitesMap
import fly.venti.core.TodayStanding
import fly.venti.core.sitesMap
import fly.venti.core.sitesPath
import fly.venti.core.todayArea
import fly.venti.core.todayOrder
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * The Today tab's take-offs against the real core, with the server played
 * by the answers the real server's own tests keep: the sites east of the
 * Tegernsee, and a forecast. Which take-offs are listed and in which order
 * is the core's and tested there; here it is that the model asks the core,
 * and what it does with the answer.
 */
class TodayModelTest {
    private val tegernsee = Coordinate(lat = 47.6, lon = 11.75)

    private fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

    private class Clock(var now: Long = 1_795_000_000_000)

    /** The pilot's own take-offs, kept by the real core in a folder of the test's own. */
    private fun keptSites(): KeptSitesModel =
        LogbookModel.open(TestSupport.newFolder("today"), TestSupport.files).makeKeptSites()

    /** The sites of the recorded answer, as the core reads them for the area it asks Today's take-offs of. */
    private fun recordedMap(around: Coordinate): SitesMap {
        val area = todayArea(around.lat, around.lon)
        return sitesMap(area.south, area.west, area.north, area.east, 200.toUShort(), recorded("sites.json"))
    }

    private fun model(
        clock: Clock = Clock(),
        kept: KeptSitesModel = keptSites(),
        answer: (String) -> HttpAnswer,
    ): TodayModel =
        TodayModel(
            kept = kept,
            transport = { url -> answer(url) },
            deviceClock = { clock.now },
            background = Dispatchers.Unconfined,
            utcOffsetSeconds = { 3600 },
        ).apply {
            server = { "https://venti.example" }
            // The moment of the recorded forecast's first hour.
            this.clock = { 1_795_000_000 }
            forecastPauseMilliseconds = 0
        }

    private fun recordedServer(url: String): HttpAnswer = when {
        "/v1/sites?" in url -> HttpAnswer(200, recorded("sites.json"))
        "/v1/forecast?" in url -> HttpAnswer(200, recorded("forecast.json"))
        else -> HttpAnswer(404, ByteArray(0))
    }

    @Test
    fun theTakeoffsAreThoseTheCoreListsForTheAreaItNames() = runBlocking {
        val asked = CopyOnWriteArrayList<String>()
        val kept = keptSites()
        val today = model(kept = kept) { url ->
            asked += url
            recordedServer(url)
        }
        today.load(tegernsee)

        assertEquals(TodayState.Loaded, today.state)
        // The sites were asked for in the box the core names for the place.
        val area = todayArea(tegernsee.lat, tegernsee.lon)
        assertEquals(
            "https://venti.example" + sitesPath(area.south, area.west, area.north, area.east),
            asked.first { "/v1/sites?" in it },
        )
        // Which take-offs, and in which order, is what the core says of that answer.
        val listed = kept.todayTakeoffs(tegernsee, recordedMap(tegernsee))
        assertEquals(3, listed.takeoffs.size)
        assertEquals(listed.takeoffs.map { it.site.id }, today.places.map { it.site.id })
        assertEquals(listed.takeoffs.map { it.distanceM }, today.places.map { it.distanceM })
        assertTrue(today.places.none { it.isKept })
        assertEquals(listed.keptTitle, today.keptTitle)
        assertEquals("Within 30 km", today.nearTitle)
        assertEquals("", today.keptMore)
        assertTrue(today.siteCredits.any { "OpenStreetMap" in it.text })
        assertNotNull(today.sitesAsOf)
        // Each with what the forecast says of it, and whose forecast it is.
        assertTrue(today.places.all { it.outlook != null && it.isAsked })
        assertTrue(today.credits.isNotEmpty())
    }

    @Test
    fun aPlaceWithNoTakeoffAroundHasAnEmptyList() = runBlocking {
        val today = model(answer = ::recordedServer)
        // Munich: the take-offs east of the Tegernsee are some 50 km away,
        // which the core leaves out.
        today.load(Coordinate(lat = 48.14, lon = 11.58))

        assertEquals(TodayState.Loaded, today.state)
        assertTrue(today.places.isEmpty())
    }

    @Test
    fun aTakeoffThePilotKeepsComesFirstAlsoFromFarAwayAndGoesWhenItIsForgotten() = runBlocking {
        val asked = CopyOnWriteArrayList<String>()
        val kept = keptSites()
        val today = model(kept = kept) { url ->
            asked += url
            recordedServer(url)
        }
        val munich = Coordinate(lat = 48.14, lon = 11.58)
        today.load(munich)
        assertTrue(today.places.isEmpty())
        assertTrue(today.isFresh(munich))

        // Kept from a site's page, with the credits and the date of the answer it came from.
        val map = recordedMap(tegernsee)
        val takeoff = map.sites.first { it.isNamed && SiteKind.TAKEOFF in it.uses }
        assertTrue(kept.keep(takeoff, map.credits, map.asOfUnixS))

        // What is shown is no longer fresh; shown again, the core lists the
        // take-offs anew from the answer at hand, and only the new
        // take-off's forecast is asked for.
        assertFalse(today.isFresh(munich))
        val sitesAsked = asked.count { "/v1/sites?" in it }
        today.show(munich)
        assertEquals(sitesAsked, asked.count { "/v1/sites?" in it })
        assertEquals(1, asked.count { "/v1/forecast?" in it })
        assertEquals(listOf(takeoff.id), today.places.map { it.site.id })
        assertTrue(today.places.single().isKept)
        assertNotNull(today.places.single().outlook)
        assertEquals("Your take-offs", today.keptTitle)
        assertTrue(today.siteCredits.any { "OpenStreetMap" in it.text })
        assertTrue(today.isFresh(munich))

        // Forgotten, it is gone the next time Today is shown.
        assertTrue(kept.forget(takeoff.id))
        today.show(munich)
        assertTrue(today.places.isEmpty())
        assertEquals(TodayState.Loaded, today.state)
    }

    @Test
    fun withoutAnAnswerFromVentiThePilotsOwnTakeoffsAreStillListed() = runBlocking {
        val kept = keptSites()
        val map = recordedMap(tegernsee)
        val takeoff = map.sites.first { it.isNamed && SiteKind.TAKEOFF in it.uses }
        kept.keep(takeoff, map.credits, map.asOfUnixS)
        val today = model(kept = kept) { throw IOException("no route to host") }
        today.load(tegernsee)

        // The screen says that Venti was not reached, and lists what the phone has,
        // with the credit the take-off was kept with.
        assertTrue(today.state is TodayState.Failed)
        assertEquals(listOf(takeoff.id), today.places.map { it.site.id })
        assertTrue(today.places.single().isAsked)
        assertTrue(today.siteCredits.any { "OpenStreetMap" in it.text })
        assertFalse(today.isFresh(tegernsee))
    }

    @Test
    fun theCardIsTheTakeoffTheCoreCallsBestAndTheRowsStandInItsOrder() = runBlocking {
        val kept = keptSites()
        // The recorded forecast is one hour of a November morning and makes
        // no day; a made-up summer day does, at the moment its hours begin.
        val bigDay = ThermalForecastTest().madeUpBigDay.toByteArray()
        val today = model(kept = kept) { url ->
            if ("/v1/forecast?" in url) HttpAnswer(200, bigDay) else recordedServer(url)
        }
        today.clock = { 1_783_137_600 }
        today.load(tegernsee)
        val day = today.places.firstNotNullOfOrNull { it.outlook?.days?.firstOrNull() }
        assertNotNull(day)

        // None kept: one list, as the core orders it for the day.
        val plain = TodayList.of(today.places, day)
        val order = todayOrder(
            today.places.map { place ->
                val shown = outlookDay(place, day)
                TodayStanding(place.site.id, place.isKept, place.distanceM, shown?.verdict, shown?.thermals)
            },
        )
        assertFalse(plain.isGrouped)
        assertEquals(order.best, plain.card?.site?.id)
        assertEquals(order.ids.filter { it != order.best }, (plain.kept + plain.near).map { it.site.id })
        assertEquals(order.ids.toSet(), plain.all.map { it.site.id }.toSet())

        // One kept, an unnamed ramp the plain list has among its rows: the
        // two groups are named, and the kept one is not among the others.
        val ramp = today.places.first { !it.site.isNamed }.site
        val map = recordedMap(tegernsee)
        kept.keep(ramp, map.credits, map.asOfUnixS)
        today.show(tegernsee)
        val grouped = TodayList.of(today.places, day)
        assertTrue(grouped.isGrouped)
        assertEquals(listOf(ramp.id), (listOfNotNull(grouped.card) + grouped.kept).filter { it.isKept }.map { it.site.id })
        assertTrue(grouped.near.none { it.isKept })
        assertEquals(3, grouped.all.size)

        // While no forecast has come, the card is the first the core lists: the pilot's own.
        val waiting = TodayList.of(today.places.map { it.copy(outlook = null) }, null)
        assertEquals(ramp.id, waiting.card?.site?.id)
        assertTrue(TodayList.of(emptyList(), null).isEmpty)
    }

    @Test
    fun aForecastTheServerIsStillFetchingIsAskedForAgain() = runBlocking {
        val asked = CopyOnWriteArrayList<String>()
        val today = model { url ->
            asked += url
            if ("/v1/forecast?" in url && asked.count { "/v1/forecast?" in it } <= 3) {
                HttpAnswer(503, ByteArray(0))
            } else {
                recordedServer(url)
            }
        }
        today.load(tegernsee)

        assertTrue(today.places.all { it.outlook != null })
    }

    @Test
    fun withoutVentiTheScreenSaysItCouldNotBeReached() = runBlocking {
        val today = model(answer = ::recordedServer)
        today.server = { null }
        today.load(tegernsee)

        assertEquals(TodayState.Failed("Venti could not be reached for today's forecast."), today.state)
        assertTrue(today.places.isEmpty())
    }

    @Test
    fun aServerThatCannotBeReachedIsSaidInWords() = runBlocking {
        val today = model { throw IOException("no route to host") }
        today.load(tegernsee)

        val state = today.state
        assertTrue(state is TodayState.Failed)
        assertTrue((state as TodayState.Failed).message.startsWith("Venti could not be reached"))
    }

    @Test
    fun anAnswerIsKeptForHalfAnHourAndForThePlaceItWasFor() = runBlocking {
        val clock = Clock()
        val today = model(clock, answer = ::recordedServer)
        assertFalse(today.isFresh(tegernsee))
        today.load(tegernsee)
        assertNotNull(today.places.firstOrNull())

        assertTrue(today.isFresh(tegernsee))
        // A few hundred metres away is the same place; ten kilometres is not.
        assertTrue(today.isFresh(Coordinate(lat = 47.602, lon = 11.75)))
        assertFalse(today.isFresh(Coordinate(lat = 47.69, lon = 11.75)))
        // Nor is another server's answer.
        today.server = { "https://other.example" }
        assertFalse(today.isFresh(tegernsee))
        today.server = { "https://venti.example" }
        clock.now += 31 * 60_000L
        assertFalse(today.isFresh(tegernsee))
    }

    @Test
    fun distancesAreMeasuredOnARoundEarth() {
        // One degree of latitude is some 111 km.
        val metres = TodayModel.metresBetween(Coordinate(lat = 47.0, lon = 11.0), Coordinate(lat = 48.0, lon = 11.0))
        assertEquals(111_195.0, metres, 50.0)
    }

    /** The development server's moment, at which its foehn answer is fresh: Bolzano – Innsbruck at "likely". */
    private val foehnMoment = java.time.Instant.parse("2026-11-18T19:05:00Z").epochSecond

    private fun foehnServer(url: String): HttpAnswer =
        if ("/v1/foehn" in url) HttpAnswer(200, recorded("foehn.json")) else recordedServer(url)

    @Test
    fun foehnIsReadAgainstTheTakeoffsTodayShowsAndTheServersClock() = runBlocking {
        val asked = CopyOnWriteArrayList<String>()
        val today = model { url ->
            asked += url
            foehnServer(url)
        }
        today.clock = { foehnMoment }
        today.load(tegernsee)

        assertTrue(asked.any { it == "https://venti.example" + fly.venti.core.foehnPath() })
        val view = (today.foehn as FoehnState.Read).view
        // What the core says of the answer for the take-offs listed, at the server's time.
        val expected = fly.venti.core.foehn(
            200.toUShort(),
            recorded("foehn.json"),
            today.places.map { it.site.position },
            foehnMoment,
        )
        assertEquals(expected, view)
        val line = checkNotNull(today.foehnLine)
        assertEquals(fly.venti.core.FoehnLevel.LIKELY, line.level)
        assertEquals("bolzano-innsbruck", line.pairId)
    }

    @Test
    fun withoutTakeoffsNearTheRidgeTodayHasNoFoehnLineButTheView() = runBlocking {
        val today = model(answer = ::foehnServer)
        today.clock = { foehnMoment }
        // Munich has no take-off around, so nothing for the line to speak of.
        today.load(Coordinate(lat = 48.14, lon = 11.58))

        assertTrue(today.places.isEmpty())
        assertTrue(today.foehn is FoehnState.Read)
        assertEquals(null, today.foehnLine)
    }

    @Test
    fun foehnNotReachedIsSaidInTheViewAndTodayHasNoLine() = runBlocking {
        val today = model { url ->
            if ("/v1/foehn" in url) throw IOException("no route to host") else recordedServer(url)
        }
        today.load(tegernsee)

        assertEquals(TodayState.Loaded, today.state)
        val failed = today.foehn as FoehnState.Failed
        assertTrue(failed.message.startsWith("Venti could not be reached."))
        assertEquals(null, today.foehnLine)
    }
}
