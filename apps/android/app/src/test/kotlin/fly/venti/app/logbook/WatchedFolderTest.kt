package fly.venti.app.logbook

import fly.venti.app.TestSupport
import fly.venti.app.settings.MemoryPreferences
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * What a watched folder keeps as seen between one look into it and the
 * next, and what it says of a folder that can no longer be found.
 */
class WatchedFolderTest {
    /** A folder a test plays: there or gone, readable or not, with the files it is given. */
    private class MadeUpFolders : Folders {
        var isThere = true
        var isReadable = true
        var files: List<Pair<String, FlightFileSource>> = emptyList()
        val kept = mutableListOf<String>()
        val released = mutableListOf<String>()

        override fun keep(folder: String) {
            kept += folder
        }

        override fun release(folder: String) {
            released += folder
        }

        override fun isThere(folder: String): Boolean = isThere

        // What Android answers for a folder it no longer finds.
        override fun name(folder: String): String = if (isThere) "Tracklogs" else "The folder"

        override fun files(folder: String): List<Pair<String, FlightFileSource>> =
            if (isReadable) files else throw IOException("The folder could not be opened.")
    }

    private fun logbook(name: String): LogbookModel = LogbookModel.open(TestSupport.newFolder(name), TestSupport.files)

    private val sample get() = "2026/sample.igc|100|1" to FlightFileSource("sample.igc") { TestSupport.sampleFlight }

    @Test
    fun aFolderThatCanNoLongerBeFoundIsLostAndStillNamed() = runBlocking {
        val folders = MadeUpFolders().apply { files = listOf(sample) }
        val preferences = MemoryPreferences()
        val watched = WatchedFolder(folders, preferences, logbook("watched-lost"))
        assertNull(watched.name)
        assertFalse(watched.isLost)

        // Chosen, the folder's flight comes in and nothing is the matter.
        assertEquals(1, checkNotNull(watched.watch(FOLDER)).added.size)
        assertEquals(listOf(FOLDER), folders.kept)
        assertEquals("Tracklogs", watched.name)
        assertFalse(watched.isLost)

        // The permission taken away, the folder deleted, the app on another
        // phone: nothing comes in, and the folder is lost, not forgotten.
        folders.isThere = false
        folders.files = listOf(sample, "2026/new.igc|200|2" to FlightFileSource("new.igc") { TestSupport.sampleFlight })
        assertNull(watched.bringInNew())
        assertTrue(watched.isLost)
        assertEquals("Tracklogs", watched.name)
        assertEquals(FOLDER, preferences.watchedFolder)
        assertEquals(
            "Venti can no longer find the folder Tracklogs, so no new flights come in from it. " +
                "Choose it again, or stop bringing in from it.",
            LogbookWords.lost(checkNotNull(watched.name)),
        )
        assertEquals("Choose the folder again", LogbookWords.CHOOSE_AGAIN)
        assertEquals("Stop bringing in from Tracklogs", LogbookWords.stopWatching("Tracklogs"))

        // The app opened again: the name is the one kept, since the folder no longer says its own.
        val again = WatchedFolder(folders, preferences, logbook("watched-lost-again"))
        assertEquals("Tracklogs", again.name)
        assertFalse(again.isLost)
        assertNull(again.bringInNew())
        assertTrue(again.isLost)
    }

    @Test
    fun aLostFolderThatIsFoundAgainBringsInWhatIsNew() = runBlocking {
        val folders = MadeUpFolders()
        val watched = WatchedFolder(folders, MemoryPreferences(), logbook("watched-found"))
        assertNull(watched.watch(FOLDER))
        folders.isThere = false
        assertNull(watched.bringInNew())
        assertTrue(watched.isLost)

        // A card put back in: the notice goes and the flight comes in.
        folders.isThere = true
        folders.files = listOf(sample)
        assertEquals(1, checkNotNull(watched.bringInNew()).added.size)
        assertFalse(watched.isLost)
    }

    @Test
    fun aFolderThatIsThereButCannotBeReadJustNowIsNotLost() = runBlocking {
        val folders = MadeUpFolders().apply { isReadable = false }
        val watched = WatchedFolder(folders, MemoryPreferences(), logbook("watched-busy"))
        assertNull(watched.watch(FOLDER))
        assertFalse(watched.isLost)
        assertEquals("Tracklogs", watched.name)
    }

    @Test
    fun givingALostFolderUpOrChoosingItAgainEndsTheNotice() = runBlocking {
        val folders = MadeUpFolders().apply { isThere = false }
        val preferences = MemoryPreferences()
        preferences.watchedFolder = FOLDER
        preferences.watchedFolderName = "Tracklogs"
        val watched = WatchedFolder(folders, preferences, logbook("watched-given-up"))
        assertNull(watched.bringInNew())
        assertTrue(watched.isLost)

        // "Stop bringing in from Tracklogs".
        watched.stop()
        assertFalse(watched.isLost)
        assertNull(watched.name)
        assertEquals("", preferences.watchedFolder)
        assertEquals("", preferences.watchedFolderName)
        assertEquals(listOf(FOLDER), folders.released)
        assertNull(watched.bringInNew())
        assertFalse(watched.isLost)

        // "Choose the folder again": lost once more, then chosen.
        preferences.watchedFolder = FOLDER
        preferences.watchedFolderName = "Tracklogs"
        val lost = WatchedFolder(folders, preferences, logbook("watched-chosen-again"))
        assertNull(lost.bringInNew())
        assertTrue(lost.isLost)
        folders.isThere = true
        folders.files = listOf(sample)
        assertEquals(1, checkNotNull(lost.watch(FOLDER)).added.size)
        assertFalse(lost.isLost)
        assertEquals("Tracklogs", lost.name)
    }

    @Test
    fun theNewFilesAreSeenAndWhatLeftTheFolderIsForgotten() {
        val seen = setOf("2026/a.igc|100|1", "2026/gone.igc|50|1")
        val now = WatchedFolder.known(
            seen,
            new = listOf("2026/b.igc|200|2"),
            keep = listOf("2026/a.igc|100|1", "2026/b.igc|200|2"),
        )
        assertEquals(setOf("2026/a.igc|100|1", "2026/b.igc|200|2"), now)
    }

    @Test
    fun aFileThatChangedHasANewKeyAndIsTriedAgain() {
        val seen = setOf("a.igc|100|1")
        val now = WatchedFolder.known(seen, new = listOf("a.igc|120|5"), keep = listOf("a.igc|120|5"))
        assertEquals(setOf("a.igc|120|5"), now)
    }

    private companion object {
        /** A folder as Android names one. */
        const val FOLDER = "content://com.android.externalstorage.documents/tree/primary%3AXCTrack%2FTracklogs"
    }
}
