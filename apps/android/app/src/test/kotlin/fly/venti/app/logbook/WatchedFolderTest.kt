package fly.venti.app.logbook

import org.junit.Assert.assertEquals
import org.junit.Test

/** What a watched folder keeps as seen between one look into it and the next. */
class WatchedFolderTest {
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
}
