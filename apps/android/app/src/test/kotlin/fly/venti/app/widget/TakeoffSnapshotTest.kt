package fly.venti.app.widget

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.today.ThermalForecastTest
import fly.venti.app.today.TodayModel
import fly.venti.app.today.TodayWords
import fly.venti.core.Coordinate
import fly.venti.core.Flyability
import fly.venti.core.SiteOutlook
import fly.venti.core.SiteView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.util.concurrent.CopyOnWriteArrayList

/**
 * What the home-screen widget keeps and what it makes of it: a snapshot of
 * the core's words written and read back, one grown old, a day that has
 * passed, and a pilot without a take-off.
 */
class TakeoffSnapshotTest {
    private val zone = ZoneId.of("Europe/Berlin")

    /** A moment of a day of November 2026 on the take-off's clock; the 20th is a Friday. */
    private fun at(day: Int, time: String = "00:00"): Long =
        LocalDate.of(2026, 11, day).atTime(LocalTime.parse(time)).atZone(zone).toEpochSecond()

    private val wallberg = WidgetTakeoff("site-1", "Wallberg")

    /** Three days as the core words them, read on the Friday at 14:05. */
    private val friday = TakeoffSnapshot(
        siteId = wallberg.id,
        siteName = wallberg.name,
        days = listOf(
            SnapshotDay(at(20), "Today", Flyability.FLYABLE, "Flyable", "Good from 11:00 to 15:00"),
            SnapshotDay(at(21), "Tomorrow", Flyability.MARGINAL, "Marginal", "Marginal"),
            SnapshotDay(at(22), "Sun", Flyability.NOT_FLYABLE, "Not flyable", "Not flyable"),
        ),
        takenUnixS = at(20, "14:05"),
    )

    private fun recorded(name: String): ByteArray = TestSupport.recordedAnswers.resolve(name).readBytes()

    @Test
    fun aSnapshotIsReadBackAsItWasWritten() {
        val store = MemoryWidgetStore()
        assertNull(store.snapshot(wallberg.id))
        store.keep(friday)
        assertEquals(friday, store.snapshot(wallberg.id))
        // A newer one of the same take-off takes its place.
        val later = friday.copy(takenUnixS = at(20, "18:00"))
        store.keep(later)
        assertEquals(later, store.snapshot(wallberg.id))
    }

    @Test
    fun aTextThatCannotBeReadIsAsGoodAsNone() {
        assertNull(TakeoffSnapshot.decoded(""))
        assertNull(TakeoffSnapshot.decoded("{\"site\":\"site-1\"}"))
        assertNull(TakeoffSnapshot.decoded(friday.encoded().replace("FLYABLE", "SPLENDID")))
        val store = MemoryWidgetStore()
        store.kept[wallberg.id] = "not a snapshot"
        assertEquals(
            WidgetContent.NoForecast(wallberg.name, isReading = true),
            WidgetRules.content(wallberg, store.snapshot(wallberg.id), at(20, "15:00"), zone),
        )
    }

    /**
     * The words a snapshot carries are the core's: Today's model reads a
     * forecast with the real core, and what it hands on is what is kept,
     * word for word.
     */
    @Test
    fun whatTodaysModelReadsIsKeptInTheCoresWords() = runBlocking {
        val read = CopyOnWriteArrayList<Pair<SiteView, SiteOutlook>>()
        val asked = CopyOnWriteArrayList<String>()
        // The recorded forecast is one hour of a November morning and makes
        // no day; the made-up summer day of Today's own tests does.
        val bigDay = ThermalForecastTest().madeUpBigDay.toByteArray()
        val today = TodayModel(
            kept = LogbookModel.open(TestSupport.newFolder("widget"), TestSupport.files).makeKeptSites(),
            transport = { url ->
                asked += url
                when {
                    "/v1/sites?" in url -> HttpAnswer(200, recorded("sites.json"))
                    "/v1/forecast?" in url -> HttpAnswer(200, bigDay)
                    else -> HttpAnswer(404, ByteArray(0))
                }
            },
            deviceClock = { 1_795_000_000_000 },
            background = Dispatchers.Unconfined,
            utcOffsetSeconds = { 3600 },
        ).apply {
            server = { "https://venti.example" }
            clock = { 1_783_137_600 }
            forecastPauseMilliseconds = 0
            onOutlook = { site, outlook -> read += site to outlook }
        }
        today.load(Coordinate(lat = 47.6, lon = 11.75))
        // Every take-off's outlook was handed on as it was read.
        assertTrue(today.places.isNotEmpty())
        assertEquals(today.places.map { it.site.id }.toSet(), read.map { it.first.id }.toSet())
        val (site, outlook) = read.first { it.second.days.isNotEmpty() }

        val store = MemoryWidgetStore()
        store.keep(TakeoffSnapshot.of(site, outlook, takenUnixS = 1_795_000_000))
        val kept = checkNotNull(store.snapshot(site.id))
        assertEquals(site.name, kept.siteName)
        assertTrue(kept.days.first().status in setOf("Flyable", "Marginal", "Not flyable"))
        assertEquals(outlook.days.map { it.name }, kept.days.map { it.name })
        assertEquals(outlook.days.map { it.status }, kept.days.map { it.status })
        assertEquals(outlook.days.map { it.verdict }, kept.days.map { it.verdict })
        assertEquals(outlook.days.map { it.headline }, kept.days.map { it.headline })
        assertEquals(outlook.days.map { it.startUnixS }, kept.days.map { it.startUnixS })

        // Brought up to date away from the screen, the server is asked anew although an outlook is at hand.
        val before = asked.count { "/v1/forecast?" in it }
        assertNotNull(today.freshOutlook(site))
        assertEquals(before + 1, asked.count { "/v1/forecast?" in it })
        assertEquals(site.id, read.last().first.id)
        assertEquals(outlook.days.map { it.status }, read.last().second.days.map { it.status })
    }

    @Test
    fun onTheDayItWasReadTheDaysAreTheCores() {
        val shown = WidgetRules.content(wallberg, friday, at(20, "17:30"), zone)
        assertEquals(
            WidgetContent.Days(
                takeoff = "Wallberg",
                days = listOf(
                    ShownDay("Today", Flyability.FLYABLE, "Flyable", hours = "Good from 11:00 to 15:00"),
                    // A line that says no more than the status word is not said twice.
                    ShownDay("Tomorrow", Flyability.MARGINAL, "Marginal", hours = ""),
                    ShownDay("Sun", Flyability.NOT_FLYABLE, "Not flyable", hours = ""),
                ),
                takenUnixS = at(20, "14:05"),
                isTakenToday = true,
            ),
            shown,
        )
    }

    @Test
    fun aDayThatHasPassedIsDroppedAndNoneIsCalledTodayByAnOldName() {
        // The next morning, eighteen hours after it was read: not yet old.
        val shown = WidgetRules.content(wallberg, friday, at(21, "08:00"), zone) as WidgetContent.Days
        // Friday is gone, and Saturday is not "Tomorrow" any more.
        assertEquals(listOf("Sat", "Sun"), shown.days.map { it.name })
        assertEquals(listOf("Marginal", "Not flyable"), shown.days.map { it.status })
        assertFalse(shown.isTakenToday)
        assertEquals(at(20, "14:05"), shown.takenUnixS)
    }

    @Test
    fun aSnapshotOlderThanADaySaysSoInPlaceOfTheDays() {
        // A day to the minute is not older than a day.
        assertTrue(WidgetRules.content(wallberg, friday, at(21, "14:05"), zone) is WidgetContent.Days)
        assertEquals(
            WidgetContent.Stale("Wallberg", takenUnixS = at(20, "14:05")),
            WidgetRules.content(wallberg, friday, at(21, "14:06"), zone),
        )
        assertEquals(
            WidgetContent.Stale("Wallberg", takenUnixS = at(20, "14:05")),
            WidgetRules.content(wallberg, friday, at(27), zone),
        )
    }

    @Test
    fun withEveryDayPassedThereIsNoForecastToShow() {
        // Read late on its last day, looked at early on the next.
        val sunday = friday.copy(takenUnixS = at(22, "23:00"))
        assertEquals(
            WidgetContent.NoForecast("Wallberg", isReading = false),
            WidgetRules.content(wallberg, sunday, at(23, "06:00"), zone),
        )
    }

    @Test
    fun withoutATakeoffTheWidgetSaysSo() {
        assertNull(WidgetRules.takeoff(chosen = null, kept = emptyList()))
        assertNull(WidgetRules.takeoff(chosen = "site-1", kept = emptyList()))
        assertEquals(WidgetContent.NoTakeoff, WidgetRules.content(null, friday, at(20, "15:00"), zone))
        assertEquals(WidgetContent.NoTakeoff, WidgetRules.content(null, null, at(20, "15:00"), zone))
    }

    @Test
    fun theTakeoffIsTheOneChosenElseThePilotsFirst() {
        val jaegerkamp = WidgetTakeoff("site-2", "Jägerkamp")
        val kept = listOf(jaegerkamp, wallberg)
        assertEquals(jaegerkamp, WidgetRules.takeoff(chosen = null, kept = kept))
        assertEquals(wallberg, WidgetRules.takeoff(chosen = "site-1", kept = kept))
        // One the pilot has forgotten since gives way to the first.
        assertEquals(jaegerkamp, WidgetRules.takeoff(chosen = "site-9", kept = kept))

        val store = MemoryWidgetStore()
        store.choose(7, wallberg.id)
        assertEquals(wallberg, WidgetRules.takeoff(store.chosen(7), kept))
        store.forget(7)
        assertEquals(jaegerkamp, WidgetRules.takeoff(store.chosen(7), kept))

        // A take-off whose forecast was never read is being read; another's snapshot is not shown as its own.
        assertEquals(
            WidgetContent.NoForecast("Jägerkamp", isReading = true),
            WidgetRules.content(jaegerkamp, friday, at(20, "15:00"), zone),
        )
    }

    @Test
    fun theWidgetShowsNoDayBeyondThoseTodayOffers() {
        val long = friday.copy(
            days = friday.days +
                SnapshotDay(at(23), "Mon", Flyability.FLYABLE, "Flyable", "Good all day") +
                SnapshotDay(at(24), "Tue", Flyability.FLYABLE, "Flyable", "Good all day"),
        )
        val shown = WidgetRules.content(wallberg, long, at(20, "15:00"), zone) as WidgetContent.Days
        assertEquals(WidgetRules.MOST_DAYS, shown.days.size)
        assertTrue(WidgetRules.MOST_DAYS <= TodayWords.DAYS)
        assertEquals("Today", shown.days.first().name)
    }
}
