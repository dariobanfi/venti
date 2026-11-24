package fly.venti.app.alerts

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.core.PilotsAlarmNumbers
import fly.venti.core.pilotsAlarmNumbers
import fly.venti.core.AlertDayKind
import fly.venti.core.AlertDraft
import fly.venti.core.AlertFaultView
import fly.venti.core.AlertKind
import fly.venti.core.AlertView
import fly.venti.core.Core
import fly.venti.core.FiredLine
import fly.venti.core.alertAddRequest
import fly.venti.core.alertChangeRequest
import fly.venti.core.alertCheck
import fly.venti.core.alertLine
import fly.venti.core.alertPoints
import fly.venti.core.alertRemoveRequest
import fly.venti.core.alertRemoved
import fly.venti.core.alertSaved
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The pilot's alarms, as the screens see them: the list and what fired as
 * the phone last fetched them, and saving, switching and removing one.
 *
 * No rule of an alarm is here: the core gives the choices, the line a
 * draft makes, what is wrong with it, every request and the reading of
 * every answer. The alarms live on the account; the phone keeps the
 * core's copy, so that the list shows without a network.
 */
class AlertsModel(
    private val core: Core,
    private val account: AccountModel,
    /** This device's clock, in milliseconds since 1970. */
    private val clock: () -> Long = System::currentTimeMillis,
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** The account's alarms, the oldest first, as last fetched. */
    var held: List<AlertView> by mutableStateOf(emptyList())
        private set

    /** What they fired in the last 30 days, the newest first, in the core's lines. */
    var fired: List<FiredLine> by mutableStateOf(emptyList())
        private set

    /** The sixteen points of the compass the directions are chosen from, from the north clockwise. */
    val points: List<String> by lazy { alertPoints() }

    /** Whether the pilot is signed in: alarms live on the account. */
    val isSignedIn: Boolean get() = account.isSignedIn

    /** Reads the list and what fired from what the phone holds. */
    /** The marker of what is new, moved when what fired was shown. */
    var unread: fly.venti.app.community.UnreadModel? = null

    suspend fun read() {
        val now = clock()
        val (list, lines) = withContext(background) {
            val list = runCatching { core.alertsHeld() }.getOrDefault(emptyList())
            val lines = runCatching { core.alertsFiredHeld(now / 1000, AccountModel.utcOffsetSeconds(now)) }
                .getOrDefault(emptyList())
            list to lines
        }
        held = list
        fired = lines
    }

    /** Fetches the list and what fired from the account, and reads it. */
    suspend fun fetch() {
        account.syncAlerts()
        read()
    }

    /** What a draft waits for, in the core's one line. */
    fun line(draft: AlertDraft): String = alertLine(draft)

    /** What is wrong with a draft, field by field; empty when it can be sent. */
    fun faults(draft: AlertDraft): List<AlertFaultView> = alertCheck(draft)

    /**
     * Saves a draft: a new alarm without `id`, a change of that alarm with
     * it. `null` when it is saved; otherwise why not, in words for the
     * pilot. The list is fetched again after it.
     */
    suspend fun save(id: String?, draft: AlertDraft): String? = send {
        val request = withContext(background) { if (id == null) alertAddRequest(draft) else alertChangeRequest(id, draft) }
        val answer = account.answer(request)
        withContext(background) { alertSaved(answer.status16, answer.body) }
    }

    /** Switches an alarm on or off. `null` when done; otherwise why not. */
    suspend fun switch(alert: AlertView, on: Boolean): String? {
        val draft = alert.draft ?: return null
        return save(alert.id, draft.copy(enabled = on))
    }

    /** Removes an alarm. `null` when it is gone; otherwise why not. */
    suspend fun remove(id: String): String? = send {
        val answer = account.answer(withContext(background) { alertRemoveRequest(id) })
        // The core reads the removal: an alarm already gone is removed too.
        withContext(background) { alertRemoved(answer.status16, answer.body) }
    }

    private suspend fun send(request: suspend () -> Unit): String? = try {
        request()
        fetch()
        null
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        account.words(error)
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** A new wind alarm for a station, by its page's identifier and name: nothing chosen yet. */
        fun windDraft(stationId: String, stationName: String) = AlertDraft(
            name = stationName,
            enabled = true,
            kind = AlertKind.WIND,
            stationId = stationId,
            stationName = stationName,
            fromPoint = null,
            toPoint = null,
            minKmh = null,
            maxKmh = null,
            gustsMaxKmh = null,
            fromHour = null,
            toHour = null,
            lat = 0.0,
            lon = 0.0,
            siteId = null,
            placeName = "",
            daysAhead = ByteArray(0),
            atLeast = AlertDayKind.CROSS_COUNTRY,
        )

        /** The two numbers of an alarm for pilots in the air: where they start, and how far they go, the core's. */
        val NUMBERS: PilotsAlarmNumbers by lazy { pilotsAlarmNumbers() }

        /**
         * A new alarm for a place as the other of its two kinds: the place
         * and the name stay; an alarm for pilots starts at three pilots
         * and six hours of quiet.
         */
        fun ofKind(draft: AlertDraft, kind: AlertKind): AlertDraft = when (kind) {
            AlertKind.PILOTS -> draft.copy(
                kind = kind,
                minPilots = draft.minPilots ?: NUMBERS.minPilotsDefault,
                restHours = draft.restHours ?: NUMBERS.restHoursDefault,
            )
            else -> draft.copy(kind = kind)
        }

        /** A new day alarm for a take-off: today and tomorrow, a cross-country day or better. */
        fun dayDraft(siteId: String, name: String, lat: Double, lon: Double) = windDraft("", "").copy(
            name = name,
            kind = AlertKind.DAY,
            lat = lat,
            lon = lon,
            siteId = siteId,
            placeName = name,
            daysAhead = byteArrayOf(0, 1),
        )
    }
}
