package fly.venti.app.forecast

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.CoreException
import fly.venti.core.ThermalHoursLayer
import fly.venti.core.ThermalMap
import fly.venti.core.ThermalMapHourSquare
import fly.venti.core.ThermalMapHours
import fly.venti.core.ThermalMapHoursView
import fly.venti.core.ThermalMapShapes
import fly.venti.core.thermalHoursPath
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** What the card's line of hours has to show for the layer chosen. */
sealed interface ThermalHoursState {
    /** The layer is the days alone: "Thermals" on its "Day", or a map without hours. */
    data object Days : ThermalHoursState

    /** The layer of the day is being asked for. */
    data object Asking : ThermalHoursState

    /** A layer of a day, read by the core. */
    data class There(val hours: ThermalMapHours, val view: ThermalMapHoursView) : ThermalHoursState

    /** The server refused or was not reached; the layer falls back to the day. */
    data class Failed(val message: String) : ThermalHoursState
}

/**
 * The hours of every day of a map for one layer, for the map's one line of
 * time: each day's view where it was read, `null` where not (yet).
 */
class ThermalHoursLine(
    val map: ThermalMap,
    val layer: ThermalHoursLayer,
    val heightM: Int?,
    val days: List<ThermalMapHoursView?>,
)

/**
 * The forecast map by the hour: for the day, the layer and the level the
 * pilot chose, the server's answer read by the core (`thermalHoursPath`,
 * `ThermalMapHours`), and the hour looked at.
 *
 * What to ask, the hours, the squares, the arrows and every word are the
 * core's. This class asks, keeps each answer for this run of the app while
 * the map of the days it belongs to is the one at hand (one answer per
 * path: wind at the ground and gusts share theirs), closes the core's
 * objects when they are let go, and remembers the hour the pilot chose
 * across the layers and heights of a day.
 */
class ThermalHoursModel(
    private val transport: WindTransport,
    /** Where the core reads an answer: a whole region's hour is a few hundred kilobytes. */
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The layer chosen. Kept by the app as the pilot left it. */
    var layer: ThermalHoursLayer by mutableStateOf(ThermalHoursLayer.THERMALS)

    /** The height of the wind chosen, metres above the sea; `null` for the ground. */
    var heightM: Int? by mutableStateOf(null)

    var state: ThermalHoursState
        get() = stateNow
        private set(value) {
            stateNow = value
            refreshShapes()
        }
    private var stateNow: ThermalHoursState by mutableStateOf(ThermalHoursState.Days)

    /**
     * The hour looked at, by its place in the day's hours; `null` on
     * "Thermals" for the day as a whole ("Day"). Set by the pilot, on the
     * line of hours. Not kept from one run of the app to the next.
     */
    var hour: Int?
        get() = hourNow
        set(value) {
            picked = value
            if (value == hourNow) return
            hourNow = value
            refreshShapes()
        }
    private var hourNow: Int? by mutableStateOf(null)

    /**
     * The hour the pilot chose, which stays across a change of layer or
     * of height; `null` while none was chosen, or "Day" was: a layer then
     * opens where it opens by itself.
     */
    private var picked: Int? = null

    /** The day the hour chosen belongs to: another day opens at its own first hour. */
    private var pickedOn: String? = null

    /** The map of the days the kept answers belong to. */
    private var keptFor: ThermalMap? = null

    /** The server's answers, by path. */
    private val answers = mutableMapOf<String, HttpAnswer>()

    /** The core's reading of an answer, by path and layer: wind and gusts read one answer twice. */
    private val read = mutableMapOf<Pair<String, ThermalHoursLayer>, ThermalMapHours>()

    /** What was asked last, so that an answer that comes late for another choice is not shown. */
    private var wanted: Pair<String, ThermalHoursLayer>? = null

    /**
     * The shapes of the hour looked at, for the map; `null` while none is
     * drawn. Asked of the core once for each change of hour: they are a
     * few hundred kilobytes of text for a whole region.
     */
    var shapes: ThermalMapShapes? by mutableStateOf(null)
        private set

    /** The hours of every day for the layer chosen, for the map's line of time; `null` until read. */
    var line: ThermalHoursLine? by mutableStateOf(null)
        private set

    /** How long the core took for the last hour's shapes, milliseconds: for the tests and the report. */
    var lastStepMillis: Long = 0
        private set

    private fun refreshShapes() {
        val there = stateNow as? ThermalHoursState.There
        val index = hourNow
        shapes = if (there == null || index == null) {
            null
        } else {
            val began = System.nanoTime()
            there.hours.shapes(index.toUInt()).also { lastStepMillis = (System.nanoTime() - began) / 1_000_000 }
        }
    }

    /**
     * Shows the chosen layer of a day: from what was kept, or from the
     * server. `server` is the address without a slash at its end; `now`
     * the moment by Venti's clock; `offset` how far the pilot's clock is
     * ahead of UTC.
     */
    suspend fun show(map: ThermalMap, date: String, server: String, now: Long, offset: Int) {
        if (map !== keptFor) forget(map)
        // A server without hours: the layer is the days alone, as before.
        if (map.layers.isEmpty()) {
            if (state !is ThermalHoursState.Days) state = ThermalHoursState.Days
            return
        }
        val path = try {
            thermalHoursPath(date, layer, heightM.takeIf { layer == ThermalHoursLayer.WIND })
        } catch (error: CoreException) {
            state = ThermalHoursState.Failed(error.pilotMessage)
            return
        }
        val key = path to layer
        wanted = key
        // Another day opens at its own first hour, whatever was chosen on the day before.
        if (pickedOn != date) {
            picked = null
            pickedOn = date
        }
        read[key]?.let { kept ->
            settle(kept)
            return
        }
        // The squares of the layer before are taken away, not left under another key.
        state = ThermalHoursState.Asking
        try {
            val answer = answers[path] ?: transport(server + path).also { answers[path] = it }
            val hours = withContext(background) {
                ThermalMapHours.read(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body, layer, offset, now)
            }
            if (map !== keptFor) {
                hours.close()
                return
            }
            read[key] = hours
            if (wanted != key) return
            settle(hours)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            // Not kept: "Try again" asks the server anew.
            answers.remove(path)
            if (wanted != key) return
            state = ThermalHoursState.Failed(
                if (error is CoreException) error.pilotMessage else "${ThermalMapWords.UNREACHABLE} ${error.pilotMessage}",
            )
            if (layer == ThermalHoursLayer.THERMALS) hour = null
        }
    }

    /**
     * Shows a layer read: at the hour the pilot chose on this day, in
     * whatever layer; without one, "Thermals" on the day as a whole,
     * "Day", and the others at the hour the core names first.
     */
    private fun settle(hours: ThermalMapHours) {
        val view = hours.view()
        val last = view.hours.lastIndex
        val chosen = picked
        hourNow = when {
            last < 0 -> null
            chosen != null -> chosen.coerceIn(0, last)
            layer == ThermalHoursLayer.THERMALS -> null
            else -> view.firstHour.toInt().coerceIn(0, last)
        }
        state = ThermalHoursState.There(hours, view)
    }

    /**
     * Reads the chosen layer of every day of the map, for the line of time
     * from what was measured to the forecast's last day: from what was
     * kept, or from the server, day by day, the line growing as each comes.
     * A day that cannot be had stays out of the line.
     */
    suspend fun loadLine(map: ThermalMap, server: String, now: Long, offset: Int) {
        if (map !== keptFor) forget(map)
        if (map.layers.isEmpty()) {
            line = null
            return
        }
        val layer = layer
        val height = heightM.takeIf { layer == ThermalHoursLayer.WIND }
        val same = line?.takeIf { it.map === map && it.layer == layer && it.heightM == height }
        val views: MutableList<ThermalMapHoursView?> = same?.days?.toMutableList() ?: MutableList(map.days.size) { null }
        if (same == null) line = ThermalHoursLine(map, layer, height, views.toList())
        for ((index, day) in map.days.withIndex()) {
            if (views.getOrNull(index) != null) continue
            val path = try {
                thermalHoursPath(day.date, layer, height)
            } catch (error: CoreException) {
                continue
            }
            val key = path to layer
            val hours = read[key] ?: try {
                val answer = answers[path] ?: transport(server + path).also { answers[path] = it }
                val reading = withContext(background) {
                    ThermalMapHours.read(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body, layer, offset, now)
                }
                if (map !== keptFor) {
                    reading.close()
                    return
                }
                // The day's own asking may have read it meanwhile: one reading is kept.
                read[key]?.also { reading.close() } ?: reading.also { read[key] = it }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                answers.remove(path)
                null
            }
            // Another layer or height was chosen meanwhile: its own line is asked for.
            if (this.layer != layer || heightM.takeIf { layer == ThermalHoursLayer.WIND } != height) return
            views[index] = hours?.view() ?: continue
            line = ThermalHoursLine(map, layer, height, views.toList())
        }
    }

    /**
     * The pilot chose an hour of a day on the line of time, or on
     * "Thermals" the day as a whole (`null`): shown at once where that
     * day's layer is read, else once it is.
     */
    fun pick(date: String, hour: Int?) {
        picked = hour
        pickedOn = date
        val there = stateNow as? ThermalHoursState.There ?: return
        if (there.view.date != date) return
        val last = there.view.hours.lastIndex
        val index = hour?.coerceIn(0, last)?.takeIf { last >= 0 }
        if (index == hourNow) return
        hourNow = index
        refreshShapes()
    }

    /** Forgets the hour the pilot chose: another "what" opens where its line begins. */
    fun unpick() {
        picked = null
        pickedOn = null
    }

    /** Drops what was kept for another map of the days, and closes the core's objects. */
    private fun forget(map: ThermalMap?) {
        read.values.forEach { it.close() }
        read.clear()
        answers.clear()
        keptFor = map
        wanted = null
        line = null
        state = ThermalHoursState.Days
        if (layer == ThermalHoursLayer.THERMALS) hour = null
        picked = null
        pickedOn = null
    }

    /** Whether a layer of hours is what the map shows: an hour of a layer read, not the day. */
    val isOn: Boolean get() = state is ThermalHoursState.There && hour != null

    /**
     * A square of the hour looked at: the one drawn under the finger, or
     * the one the core finds at the place. `null` outside the map's grid.
     */
    fun square(cell: UInt?, lat: Double, lon: Double): ThermalMapHourSquare? {
        val there = state as? ThermalHoursState.There ?: return null
        val index = hour ?: return null
        val found = cell ?: there.hours.cellAt(lat, lon) ?: return null
        return there.hours.square(found, index.toUInt())
    }

    /** The square of the hours' grid a place lies in; `null` outside it or with no hours read. */
    fun cellAt(lat: Double, lon: Double): UInt? = (state as? ThermalHoursState.There)?.hours?.cellAt(lat, lon)

    /** Lets everything go: the layer is switched off, or the server changed. */
    fun close() = forget(null)

    /** How many answers and readings are kept: for the tests. */
    val kept: Pair<Int, Int> get() = answers.size to read.size
}
