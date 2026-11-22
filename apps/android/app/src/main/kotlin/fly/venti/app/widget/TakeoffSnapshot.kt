package fly.venti.app.widget

import fly.venti.app.today.TodayWords
import fly.venti.core.Flyability
import fly.venti.core.SiteOutlook
import fly.venti.core.SiteView
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * One day of a take-off as the home-screen widget keeps it: the core's own
 * words and its status value, as Today showed them when the outlook was read.
 */
data class SnapshotDay(
    /** The day's first moment on the place's clock, seconds since 1970. */
    val startUnixS: Long,
    /** What the core called the day when the outlook was read: "Today", "Tomorrow", "Fri". */
    val name: String,
    val verdict: Flyability,
    /** The verdict in the core's word: "Flyable", "Marginal", "Not flyable". */
    val status: String,
    /** The day in the core's line, which names the flying hours: "Good from 11:00 to 15:00". */
    val headline: String,
)

/**
 * What the widget draws for a take-off: what the core said of its days, and
 * when that was read by this phone's clock. Written whenever Today's model
 * has read an outlook of one of the pilot's own take-offs; the widget draws
 * nothing else, and asks no server itself.
 */
data class TakeoffSnapshot(
    val siteId: String,
    val siteName: String,
    val days: List<SnapshotDay>,
    /** When the outlook was read, by this phone's clock, seconds since 1970. */
    val takenUnixS: Long,
) {
    /** The snapshot as the text the store keeps. */
    fun encoded(): String = buildJsonObject {
        put("site", siteId)
        put("name", siteName)
        put("taken", takenUnixS)
        put(
            "days",
            buildJsonArray {
                for (day in days) {
                    add(
                        buildJsonObject {
                            put("start", day.startUnixS)
                            put("name", day.name)
                            put("verdict", day.verdict.name)
                            put("status", day.status)
                            put("headline", day.headline)
                        },
                    )
                }
            },
        )
    }.toString()

    companion object {
        /** What an outlook says of a site, read at a moment of this phone's clock. */
        fun of(site: SiteView, outlook: SiteOutlook, takenUnixS: Long): TakeoffSnapshot = TakeoffSnapshot(
            siteId = site.id,
            siteName = site.name,
            days = outlook.days.map { SnapshotDay(it.startUnixS, it.name, it.verdict, it.status, it.headline) },
            takenUnixS = takenUnixS,
        )

        /** A kept text read back; `null` for one that cannot be read, which is then as good as none. */
        fun decoded(text: String): TakeoffSnapshot? = try {
            val all = Json.parseToJsonElement(text).jsonObject
            TakeoffSnapshot(
                siteId = all.string("site"),
                siteName = all.string("name"),
                takenUnixS = all.getValue("taken").jsonPrimitive.long,
                days = all.getValue("days").jsonArray.map { element ->
                    val day = element.jsonObject
                    SnapshotDay(
                        startUnixS = day.getValue("start").jsonPrimitive.long,
                        name = day.string("name"),
                        verdict = Flyability.valueOf(day.string("verdict")),
                        status = day.string("status"),
                        headline = day.string("headline"),
                    )
                },
            )
        } catch (_: Exception) {
            null
        }

        private fun JsonObject.string(name: String): String = getValue(name).jsonPrimitive.content
    }
}

/** One of the pilot's own take-offs, as the widget and its choice name it. */
data class WidgetTakeoff(val id: String, val name: String)

/** A day as the widget draws it. */
data class ShownDay(
    val name: String,
    val verdict: Flyability,
    val status: String,
    /** The core's line where it says more than the status word does: the flying hours. Empty otherwise. */
    val hours: String,
)

/** What the widget has to show. */
sealed interface WidgetContent {
    /** The pilot keeps no take-off. */
    data object NoTakeoff : WidgetContent

    /** A take-off with nothing to show of it: no outlook has been read yet, or none of its days is left. */
    data class NoForecast(val takeoff: String, val isReading: Boolean) : WidgetContent

    /** What was read is older than a day: no status is shown, since none of it is today's any more. */
    data class Stale(val takeoff: String, val takenUnixS: Long) : WidgetContent

    data class Days(
        val takeoff: String,
        val days: List<ShownDay>,
        val takenUnixS: Long,
        /** Whether it was read on the day it is shown on: then the hour alone says when. */
        val isTakenToday: Boolean,
    ) : WidgetContent
}

/** The widget's few rules about what is kept, none of them about flying. */
object WidgetRules {
    /** How old a snapshot may be before the widget says so in place of the days, seconds. */
    const val STALE_AFTER_S = 24 * 3600L

    /** How many days the widget shows at most in its wide size: no more than Today offers. */
    val MOST_DAYS: Int = minOf(4, TodayWords.DAYS)

    /** How many in its small size. */
    const val DAYS_WHEN_SMALL = 2

    /**
     * The take-off a widget shows: the one chosen for it while the pilot
     * still keeps it, else the first of the pilot's own.
     */
    fun takeoff(chosen: String?, kept: List<WidgetTakeoff>): WidgetTakeoff? =
        kept.firstOrNull { it.id == chosen } ?: kept.firstOrNull()

    /**
     * What to draw at a moment of this phone's clock. A snapshot older
     * than a day is said to be so. A day that has passed is left out. And
     * a day keeps the core's name for it only on the day the snapshot was
     * read: after midnight "Today" is yesterday and "Tomorrow" is today,
     * so each day is then called by its day of the week, as the core
     * calls the later days.
     */
    fun content(takeoff: WidgetTakeoff?, snapshot: TakeoffSnapshot?, nowUnixS: Long, zone: ZoneId): WidgetContent {
        if (takeoff == null) return WidgetContent.NoTakeoff
        if (snapshot == null || snapshot.siteId != takeoff.id) return WidgetContent.NoForecast(takeoff.name, isReading = true)
        if (nowUnixS - snapshot.takenUnixS > STALE_AFTER_S) return WidgetContent.Stale(takeoff.name, snapshot.takenUnixS)
        val today = date(nowUnixS, zone)
        val isTakenToday = date(snapshot.takenUnixS, zone) == today
        val days = snapshot.days
            .filter { !date(it.startUnixS, zone).isBefore(today) }
            .take(MOST_DAYS)
            .map { day ->
                ShownDay(
                    name = if (isTakenToday) day.name else WEEKDAY.format(date(day.startUnixS, zone)),
                    verdict = day.verdict,
                    status = day.status,
                    hours = day.headline.takeIf { it != day.status }.orEmpty(),
                )
            }
        if (days.isEmpty()) return WidgetContent.NoForecast(takeoff.name, isReading = false)
        return WidgetContent.Days(takeoff.name, days, snapshot.takenUnixS, isTakenToday)
    }

    private fun date(unixS: Long, zone: ZoneId): LocalDate = Instant.ofEpochSecond(unixS).atZone(zone).toLocalDate()

    /** A day of the week as the core writes one: "Fri". */
    private val WEEKDAY = DateTimeFormatter.ofPattern("EEE", Locale.ENGLISH)
}
