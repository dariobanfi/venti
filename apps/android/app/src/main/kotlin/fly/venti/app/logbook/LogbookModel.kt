package fly.venti.app.logbook

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.AppFiles
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.core.Assistant
import fly.venti.core.Core
import fly.venti.core.FlightAnalysis
import fly.venti.core.FlightView
import fly.venti.core.LogbookStatistics
import fly.venti.core.LogbookView
import fly.venti.core.PendingRecording
import fly.venti.core.clearRecording
import fly.venti.core.pendingRecordings
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException

/**
 * A flight file to import: what to call it when something is wrong with it,
 * and how to get its text. Reading is put off until the import runs, away
 * from the main thread.
 */
class FlightFileSource(val name: String, val read: () -> String)

/**
 * The pilot's logbook as the screens see it.
 *
 * The model contains no flying logic. The core stores the flights, works out
 * every number and groups them; the model asks for the result and keeps the
 * latest copy for the screens to draw.
 */
class LogbookModel(
    private val core: Core,
    /** The folder the core keeps its data in. The cockpit records into it. */
    val folder: File,
    private val files: AppFiles,
    /** Where reading and parsing run, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    var view: LogbookView by mutableStateOf(core.logbook())
        private set

    /**
     * Moves a finished recording into the logbook and returns the flight's
     * identifier. The recording is cleared only once the logbook has it, so
     * a failure here loses nothing: it is found again at the next launch.
     */
    suspend fun adopt(recording: PendingRecording): String {
        val id = withContext(background) {
            val imported = core.importFlight(recording.igcText)
            clearRecording(folder.path, recording.name)
            imported.id
        }
        view = core.logbook()
        return id
    }

    /**
     * The recordings that were cut short: flights being recorded when the
     * app was closed, or when it crashed. Listed once, at launch, before a
     * new recording can exist.
     */
    fun interruptedRecordings(): List<PendingRecording> = pendingRecordings(folder.path)

    /** Brings interrupted recordings into the logbook. */
    suspend fun adopt(recordings: List<PendingRecording>) {
        for (recording in recordings) {
            runCatching { adopt(recording) }
        }
    }

    /** Imports flight files. */
    suspend fun importFiles(sources: List<FlightFileSource>): ImportReport {
        val report = withContext(background) {
            val added = mutableListOf<String>()
            val alreadyThere = mutableListOf<String>()
            val failures = mutableListOf<ImportReport.Failure>()
            for (source in sources) {
                try {
                    val imported = core.importFlight(source.read())
                    (if (imported.alreadyInLogbook) alreadyThere else added) += imported.id
                } catch (error: Exception) {
                    failures += ImportReport.Failure(file = source.name, reason = error.pilotMessage)
                }
            }
            ImportReport(added, alreadyThere, failures)
        }
        view = core.logbook()
        return report
    }

    /**
     * Imports the made-up flight shipped with the app, so a new pilot has
     * something to look at.
     */
    suspend fun addSample(): ImportReport =
        importFiles(listOf(FlightFileSource(AppFiles.SAMPLE_FLIGHT) { files.text(AppFiles.SAMPLE_FLIGHT) }))

    /** One flight, prepared for display. */
    suspend fun flight(id: String): FlightView = withContext(background) { core.flight(id) }

    /** What happened in one flight: thermals, glides, distance. */
    suspend fun analysis(id: String): FlightAnalysis = withContext(background) { core.flightAnalysis(id) }

    /**
     * Writes a flight's file into `folder`, to be shared or saved elsewhere,
     * and returns where it is.
     */
    suspend fun exportFile(id: String, into: File): File = withContext(background) {
        val file = core.flightFile(id)
        into.mkdirs()
        into.resolve(file.name).also { it.writeText(file.igcText) }
    }

    /** A new conversation with the assistant, which answers from this logbook. */
    fun makeAssistant(model: String): Assistant = core.assistant(model)

    /** The pilot's waypoints, which the core keeps beside the logbook. */
    fun makeWaypoints(): WaypointsModel = WaypointsModel(core, background)

    /**
     * The pilot's airspace, which the core keeps there too: the pilot's
     * files, and what was fetched from the pilot's server. `now` is the
     * clock a file's day of arrival and the age of what is kept are read
     * from; `remembered` keeps the choice to use the server; requests to
     * the server run in `scope` and go out through `transport`.
     */
    fun makeAirspace(
        now: () -> Long = { System.currentTimeMillis() / 1000 },
        remembered: Preferences? = null,
        scope: CoroutineScope? = null,
        transport: suspend (String) -> HttpAnswer = { url -> Http.get(url) },
    ): AirspaceModel = AirspaceModel(core, background, now, remembered, scope, transport)

    /** The logbook looked at as a whole: months, gliders, bests. */
    fun statistics(): LogbookStatistics = core.statistics()

    /** Removes a flight for good. */
    fun delete(id: String) {
        try {
            core.deleteFlight(id)
        } finally {
            view = core.logbook()
        }
    }

    companion object {
        /** Opens the logbook kept in `folder`, creating it on first use. */
        fun open(folder: File, files: AppFiles): LogbookModel = LogbookModel(Core(folder.path), folder, files)

        /**
         * The text of a flight file. They are plain ASCII by specification,
         * but recorders write pilot names in whatever encoding they like:
         * bytes that are not UTF-8 are read as Latin-1, which keeps every
         * line in place.
         */
        fun decode(bytes: ByteArray): String {
            return try {
                Charsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(bytes)).toString()
            } catch (_: CharacterCodingException) {
                bytes.toString(Charsets.ISO_8859_1)
            }
        }
    }
}
