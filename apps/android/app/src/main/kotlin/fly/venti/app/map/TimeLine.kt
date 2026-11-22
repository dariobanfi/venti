package fly.venti.app.map

import fly.venti.core.RadarMoment
import fly.venti.core.RadarView
import fly.venti.core.ThermalHoursLayer
import fly.venti.core.ThermalMapDay
import fly.venti.core.ThermalMapHoursView

/**
 * What the map tab shows over the ground, one at a time (the owner,
 * 2026-11-22, "One choice of what the map shows" in docs/design.md): the
 * wind, the thermals, the rain or the cloud. The wind until the pilot
 * chooses; the one chosen pressed again shows nothing.
 */
enum class MapWhat(val key: String) {
    WIND("wind"),
    THERMALS("thermals"),
    RAIN("rain"),
    CLOUD("cloud");

    /** The forecast's layer of hours it is, the wind's mean or its gusts. */
    fun layer(gusts: Boolean): ThermalHoursLayer = when (this) {
        WIND -> if (gusts) ThermalHoursLayer.GUSTS else ThermalHoursLayer.WIND
        THERMALS -> ThermalHoursLayer.THERMALS
        RAIN -> ThermalHoursLayer.RAIN
        CLOUD -> ThermalHoursLayer.CLOUD
    }

    /** Whether its line opens on what was measured: the wind stations, the radar's newest picture. */
    val opensMeasured: Boolean get() = this == WIND || this == RAIN

    companion object {
        /** What is kept for "nothing": not the same as nothing kept, which is the wind. */
        const val NONE = "none"

        /** What the pilot left, as kept: the wind until they chose. */
        fun read(kept: String?): MapWhat? = if (kept == null) WIND else entries.firstOrNull { it.key == kept }

        /** What to keep for a choice. */
        fun kept(what: MapWhat?): String = what?.key ?: NONE
    }
}

/** A stop of the line of time: what the map draws while the line stands there. */
sealed interface Stop {
    /** One of the radar's moments, by its place in the radar's list. */
    data class Radar(val moment: Int) : Stop

    /** The wind stations, as they measure now. */
    data object Stations : Stop

    /** A day of the forecast as a whole: the kind of day, square by square. */
    data class Day(val date: String) : Stop

    /** An hour of the forecast, by its place in its day's hours. */
    data class Hour(val date: String, val hour: Int) : Stop
}

/** A cell of the line of time. */
data class LineCell(
    val stop: Stop,
    /** What the cell shows: the clock's hour, "13", "Day", or nothing. */
    val short: String,
    /** What TalkBack says of it. */
    val label: String,
    /** Whether it was measured, not forecast. */
    val measured: Boolean,
    /** What stands over the cell: a day's name over its first, a radar's full hour, "Now". */
    val over: String? = null,
)

/**
 * The map's one line of time for what it shows: from what was measured in
 * the last hours, the radar's pictures or the wind stations of now, through
 * "Now" to the forecast's hours, day after day. Every time and word in it is
 * the core's (the radar's moments, the forecast map's days and hours, the
 * map's words); this only lays them along one line, as the web's
 * `timeline.ts` does.
 */
object TimeLine {
    /**
     * The cells, the oldest first: what was measured, then each day of the
     * forecast, today's from the hour that has begun. The thermals have the
     * day as a whole before each day's hours.
     */
    fun cells(
        what: MapWhat,
        /** The radar's list, for the rain. */
        radar: RadarView?,
        /** Whether the wind stations can be shown, for the wind. */
        stations: Boolean,
        /** The forecast map's days, today first. */
        days: List<ThermalMapDay>,
        /** The hours of each of those days for the layer shown, where they were read. */
        hours: List<ThermalMapHoursView?>?,
        /** "Now", the core's. */
        now: String,
        /** The cell of a day as a whole on the thermals' line: "Day". */
        dayWord: String,
    ): List<LineCell> {
        val measured = mutableListOf<LineCell>()
        if (what == MapWhat.RAIN) {
            val moments = radar?.moments.orEmpty()
            moments.forEachIndexed { index, moment ->
                measured += LineCell(Stop.Radar(index), "", moment.label, measured = true, over = radarOver(moments, index))
            }
        }
        val withStations = what == MapWhat.WIND && stations
        if (withStations) {
            measured += LineCell(Stop.Stations, "", now, measured = true, over = now)
        }
        val forecast = mutableListOf<LineCell>()
        days.forEachIndexed { index, day ->
            val view = hours?.getOrNull(index)
            val ofDay = mutableListOf<LineCell>()
            if (what == MapWhat.THERMALS) {
                ofDay += LineCell(Stop.Day(day.date), dayWord, day.name, measured = false)
            }
            // Today from the hour that has begun: what came before it was
            // measured or is past. Which day is today the core says.
            val from = if (day.today) (view?.firstHour?.toInt() ?: 0).coerceAtLeast(0) else 0
            view?.hours?.forEachIndexed { hour, mark ->
                if (hour < from) return@forEachIndexed
                // "Now" where the measured meets the forecast: over the stations for the wind.
                val isNow = day.today && hour == from && !withStations
                ofDay += LineCell(
                    Stop.Hour(day.date, hour),
                    mark.clockHour.toString(),
                    "${day.name}, ${mark.label}",
                    measured = false,
                    over = now.takeIf { isNow },
                )
            }
            // A day's name over its first cell, where "Now" does not stand on it.
            if (ofDay.isNotEmpty() && ofDay.none { it.over == now }) {
                ofDay[0] = ofDay[0].copy(over = day.name)
            }
            forecast += ofDay
        }
        return measured + forecast
    }

    /**
     * The time over a radar's moment: a full hour's, and the first
     * moment's where no full hour follows soon. A short run of moments has
     * none, its cells too narrow for the words, which the panel's line says.
     */
    private fun radarOver(moments: List<RadarMoment>, index: Int): String? {
        if (moments.size < SOON) return null
        val clock = moments[index].clock
        val first = index == 0 && moments.drop(1).take(SOON - 1).none { it.clock.endsWith(":00") }
        return clock.takeIf { it.endsWith(":00") || first }
    }

    /** How many moments make a run long enough for the times over them. */
    private const val SOON = 6
}
