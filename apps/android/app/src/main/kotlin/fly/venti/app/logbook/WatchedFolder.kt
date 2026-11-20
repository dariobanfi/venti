package fly.venti.app.logbook

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.net.toUri
import fly.venti.app.settings.Preferences
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The bridge for pilots who fly with another app: a folder the pilot chooses
 * once, which Venti looks into again whenever it comes to the front, bringing
 * in the flight files that are new there. XCTrack's tracklog folder is the
 * one it is for: the pilot flies as before, and the flight is in Venti's
 * logbook by itself.
 *
 * Android keeps the permission to read the folder across restarts. A file is
 * known by where it lies, its size and when it was last changed, so only
 * what is new or changed is read; the core's logbook recognises a flight it
 * has in any case.
 */
class WatchedFolder(
    private val context: Context,
    private val preferences: Preferences,
    private val logbook: LogbookModel,
) {
    /** The folder's name as the pilot knows it, while one is watched. */
    var name: String? by mutableStateOf(null)
        private set

    init {
        name = folder()?.let { FlightFiles.folderName(context, it) }
    }

    private fun folder(): Uri? = preferences.watchedFolder.takeIf { it.isNotEmpty() }?.toUri()

    /** Watches a folder the pilot chose, in place of any other, and brings in what is in it. */
    suspend fun watch(tree: Uri): ImportReport? {
        stop()
        context.contentResolver.takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        preferences.watchedFolder = tree.toString()
        preferences.watchedSeen = emptySet()
        name = FlightFiles.folderName(context, tree)
        return bringInNew()
    }

    /** No folder is watched any more; the flights brought in stay. */
    fun stop() {
        folder()?.let { tree ->
            runCatching { context.contentResolver.releasePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
        }
        preferences.watchedFolder = ""
        preferences.watchedSeen = emptySet()
        name = null
    }

    /**
     * Brings in the flight files of the watched folder that are new since it
     * was last looked into. `null` when nothing was new, no folder is
     * watched, or the folder cannot be read now: then it is tried again next
     * time, and the pilot is told nothing.
     */
    suspend fun bringInNew(): ImportReport? {
        val tree = folder() ?: return null
        val allowed = context.contentResolver.persistedUriPermissions.any { it.uri == tree && it.isReadPermission }
        if (!allowed) return null
        val found = try {
            withContext(Dispatchers.IO) { FlightFiles.keyedInFolder(context, tree) }
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            return null
        }
        val seen = preferences.watchedSeen
        val new = found.filter { (key, _) -> key !in seen }
        if (new.isEmpty()) return null
        val report = logbook.importFiles(new.map { it.second })
        // What was tried is seen, a file that is not a flight included: it
        // is tried again once it has changed.
        preferences.watchedSeen = known(seen, new.map { it.first }, keep = found.map { it.first })
        return report
    }

    companion object {
        /** The keys to keep as seen: those still in the folder, and the new ones. */
        fun known(seen: Set<String>, new: List<String>, keep: List<String>): Set<String> {
            val there = keep.toSet()
            return (seen.filter { it in there } + new).toSet()
        }
    }
}
