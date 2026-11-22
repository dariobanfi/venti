package fly.venti.app.logbook

import android.content.Context
import android.content.Intent
import android.provider.DocumentsContract
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.net.toUri
import fly.venti.app.settings.Preferences
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * How the app gets into a folder the pilot picked, named as Android names
 * it (the address of its tree): Android's own way, or a test's.
 */
interface Folders {
    /** Keeps the permission to read the folder across restarts. Throws where Android gives none. */
    fun keep(folder: String)

    /** Gives that permission back. */
    fun release(folder: String)

    /**
     * Whether the folder can still be found: the app still holds the
     * permission to read it, and the folder still answers.
     */
    fun isThere(folder: String): Boolean

    /** The folder's name as the pilot knows it. */
    fun name(folder: String): String

    /** Every flight file in the folder and in the folders inside it, each with its key. Throws where it cannot be read. */
    fun files(folder: String): List<Pair<String, FlightFileSource>>
}

/** Folders through Android's documents: the permission the pilot gave in the folder picker. */
class AndroidFolders(private val context: Context) : Folders {
    override fun keep(folder: String) {
        context.contentResolver.takePersistableUriPermission(folder.toUri(), Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }

    override fun release(folder: String) {
        runCatching {
            context.contentResolver.releasePersistableUriPermission(folder.toUri(), Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
    }

    override fun isThere(folder: String): Boolean {
        val tree = folder.toUri()
        // The pilot took the permission away, or the app was restored on
        // another phone, where it never had it.
        if (context.contentResolver.persistedUriPermissions.none { it.uri == tree && it.isReadPermission }) return false
        // The permission outlives the folder: one that was deleted no longer answers.
        return runCatching {
            val itself = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
            context.contentResolver.query(itself, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID), null, null, null)
                ?.use { it.moveToFirst() } ?: false
        }.getOrDefault(false)
    }

    override fun name(folder: String): String = FlightFiles.folderName(context, folder.toUri())

    override fun files(folder: String): List<Pair<String, FlightFileSource>> = FlightFiles.keyedInFolder(context, folder.toUri())
}

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
 *
 * Where the folder can no longer be found (the pilot took the permission
 * away, the folder was deleted, the app was restored on another phone) it
 * is lost: [isLost] is set, nothing is brought in, and the logbook says so
 * and offers to choose the folder again, as on the iPhone.
 */
class WatchedFolder(
    private val folders: Folders,
    private val preferences: Preferences,
    private val logbook: LogbookModel,
    /** Where Android is asked about the folder, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** The folder's name as the pilot knows it, while one is watched. */
    var name: String? by mutableStateOf(null)
        private set

    /** Whether the watched folder can no longer be found; said in the logbook. */
    var isLost: Boolean by mutableStateOf(false)
        private set

    init {
        // The name kept when the folder was chosen: a folder that is lost
        // no longer says its own. One chosen before the name was kept is
        // asked, while it is still there to ask.
        name = folder()?.let { preferences.watchedFolderName.ifEmpty { folders.name(it) } }
    }

    private fun folder(): String? = preferences.watchedFolder.takeIf { it.isNotEmpty() }

    /**
     * Watches a folder the pilot chose, in place of any other, and brings in
     * what is in it. Throws where Android gives no lasting permission for it.
     */
    suspend fun watch(folder: String): ImportReport? {
        stop()
        folders.keep(folder)
        val shown = folders.name(folder)
        preferences.watchedFolder = folder
        preferences.watchedFolderName = shown
        preferences.watchedSeen = emptySet()
        name = shown
        return bringInNew()
    }

    /** No folder is watched any more; the flights brought in stay. */
    fun stop() {
        folder()?.let(folders::release)
        preferences.watchedFolder = ""
        preferences.watchedFolderName = ""
        preferences.watchedSeen = emptySet()
        name = null
        isLost = false
    }

    /**
     * Brings in the flight files of the watched folder that are new since it
     * was last looked into. `null` when nothing was new, no folder is
     * watched, or the folder cannot be read just now: then it is tried again
     * next time, and the pilot is told nothing. A folder that is lost is
     * said by [isLost].
     */
    suspend fun bringInNew(): ImportReport? {
        val folder = folder() ?: return null
        if (!withContext(background) { folders.isThere(folder) }) {
            isLost = true
            return null
        }
        // Found, if not read: a card that is busy, a folder another app is
        // still writing into.
        isLost = false
        val found = try {
            withContext(background) { folders.files(folder) }
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
