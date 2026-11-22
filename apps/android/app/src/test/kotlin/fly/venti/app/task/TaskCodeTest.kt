package fly.venti.app.task

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.readTask
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A task as a QR code, against the real core: the code's text is the
 * core's, its dots are drawn into a picture as the screen draws them, the
 * picture is read with the reader the camera's frames go to, and the core
 * takes what was read as the same task.
 */
class TaskCodeTest {
    private fun tasks(name: String): TaskModel = LogbookModel.open(TestSupport.newFolder(name), TestSupport.files).makeTasks()

    /** The made-up task along the sample flight. */
    private val sample = LaunchOptions.SAMPLE_TASK_FILE

    /** The dots as a picture, a few pixels to a dot, dark on white with the empty room around them. */
    private fun picture(dots: TaskCode.Dots, pixelsToADot: Int = 4): Pair<IntArray, Int> {
        val side = (dots.side + 2 * TaskCode.QUIET_DOTS) * pixelsToADot
        val pixels = IntArray(side * side) { index ->
            val column = (index % side) / pixelsToADot - TaskCode.QUIET_DOTS
            val row = (index / side) / pixelsToADot - TaskCode.QUIET_DOTS
            val inside = column in 0 until dots.side && row in 0 until dots.side
            if (inside && dots.isDark(column, row)) BLACK else WHITE
        }
        return pixels to side
    }

    private fun readBack(text: String): String? {
        val (pixels, side) = picture(checkNotNull(TaskCode.dots(text)))
        return TaskCode.read(pixels, side, side)
    }

    @Test
    fun withoutATaskThereIsNoCode() {
        val tasks = tasks("task-code-none")
        assertNull(tasks.code(compressed = true))
        assertNull(tasks.code(compressed = false))
    }

    @Test
    fun theKeptTasksCodeIsDrawnReadBackAndKeptAsTheSameTask() {
        val tasks = tasks("task-code")
        assertTrue(tasks.keep(sample))
        val kept = checkNotNull(tasks.task)

        for (compressed in listOf(true, false)) {
            val code = checkNotNull(tasks.code(compressed))
            // Drawn and read, the text is the core's to the letter.
            assertEquals(code, readBack(code))

            // Another pilot's phone takes the text it read as the same task.
            val other = tasks("task-code-other-$compressed")
            assertTrue(other.keep(code))
            assertNull(other.problem)
            val scanned = checkNotNull(other.task)
            assertEquals(kept.turnpoints.map { it.name }, scanned.turnpoints.map { it.name })
            assertEquals(kept.turnpoints.map { it.role }, scanned.turnpoints.map { it.role })
            assertEquals(kept.turnpoints.map { it.radiusM }, scanned.turnpoints.map { it.radiusM })
            assertEquals(kept.lengthM, scanned.lengthM, 1.0)
        }
        // The two kinds are two texts, and the compressed one has no more dots.
        val small = checkNotNull(tasks.code(compressed = true))
        val plain = checkNotNull(tasks.code(compressed = false))
        assertNotEquals(small, plain)
        assertTrue(checkNotNull(TaskCode.dots(small)).side <= checkNotNull(TaskCode.dots(plain)).side)
        // The core reads either without keeping it.
        assertEquals(kept.turnpoints.size, readTask(small, 0).turnpoints.size)
    }

    @Test
    fun aCodeThatIsNoTaskIsRefusedInTheCoresWordsAndTheTaskStays() {
        val tasks = tasks("task-code-refused")
        assertTrue(tasks.keep(sample))

        // A code as a shop's or a ticket's: read well, and no task.
        val read = readBack("https://example.org/not-a-task")
        assertEquals("https://example.org/not-a-task", read)
        assertFalse(tasks.keep(checkNotNull(read)))
        assertTrue(checkNotNull(tasks.problem).isNotBlank())
        assertNotNull(tasks.task)
    }

    @Test
    fun aPictureWithoutACodeReadsAsNothing() {
        val side = 200
        assertNull(TaskCode.read(IntArray(side * side) { WHITE }, side, side))
        // The camera's plane of brightness, with more bytes to a row than the picture is wide.
        val rowWidth = side + 8
        assertNull(TaskCode.read(ByteArray(rowWidth * side) { 0x7f }, rowWidth, side, side))
    }

    @Test
    fun aCameraFrameIsReadFromItsBrightness() {
        val tasks = tasks("task-code-frame")
        assertTrue(tasks.keep(sample))
        val code = checkNotNull(tasks.code(compressed = true))
        val (pixels, side) = picture(checkNotNull(TaskCode.dots(code)))
        // A frame as the camera hands it over: one byte of brightness to a
        // pixel, and rows longer than the picture is wide.
        val rowWidth = side + 16
        val brightness = ByteArray(rowWidth * side)
        for (row in 0 until side) {
            for (column in 0 until side) {
                brightness[row * rowWidth + column] = if (pixels[row * side + column] == BLACK) 0x10 else 0xf0.toByte()
            }
        }
        assertEquals(code, TaskCode.read(brightness, rowWidth, side, side))
    }

    @Test
    fun theWordsAreTheIPhones() {
        assertEquals("Show as a QR code", TaskCodeWords.SHOW)
        assertEquals("Scan a QR code", TaskCodeWords.SCAN)
        assertEquals("Task as a QR code", TaskCodeWords.TITLE)
        assertEquals("Hold the task's QR code in view.", TaskCodeWords.HOLD)
        assertEquals("The camera cannot be used", TaskCodeWords.NO_CAMERA_TITLE)
    }

    private companion object {
        const val BLACK = 0xFF000000.toInt()
        const val WHITE = 0xFFFFFFFF.toInt()
    }
}
