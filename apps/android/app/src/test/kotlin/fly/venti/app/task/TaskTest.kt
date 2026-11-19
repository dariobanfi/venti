package fly.venti.app.task

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.app.fly.Announcer
import fly.venti.app.fly.CockpitMap
import fly.venti.app.fly.FlightSensors
import fly.venti.app.fly.FlySession
import fly.venti.app.fly.SilentTone
import fly.venti.app.fly.TaskStripWords
import fly.venti.app.logbook.FlightFileSource
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.waypoints.place
import fly.venti.core.FlightSession
import fly.venti.core.TaskPhase
import fly.venti.core.TaskRole
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.IOException
import java.util.Collections
import java.util.Locale

/**
 * The pilot's task against the real core: kept from a file's text, made
 * from waypoints, and flown by a replay of the sample flight, along the
 * made-up task the UI tests start with.
 */
class TaskTest {
    private val locale = Locale.getDefault()
    private val scope = CoroutineScope(Dispatchers.Default)

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
        scope.cancel()
    }

    private fun logbook(): LogbookModel = LogbookModel.open(TestSupport.newFolder("task"), TestSupport.files)

    /** The made-up task along the sample flight. */
    private val sample = LaunchOptions.SAMPLE_TASK_FILE

    @Test
    fun aTaskIsKeptFromAFilesTextAndSaidInALine() {
        val logbook = logbook()
        val tasks = logbook.makeTasks()
        assertNull(tasks.task)
        assertNull(tasks.file)

        assertTrue(tasks.keep(sample))
        assertNull(tasks.problem)
        val task = checkNotNull(tasks.task)
        assertFalse(task.isRoute)
        assertEquals(
            listOf("Sample launch", "Sample start", "Sample turnpoint west", "Sample goal"),
            task.turnpoints.map { it.title },
        )
        assertEquals("4 turnpoints · ${fly.venti.app.design.Format.kilometres(task.lengthM)} km", task.inALine)
        assertEquals(listOf(TaskRole.LAUNCH, TaskRole.START, TaskRole.TURNPOINT, TaskRole.GOAL), task.turnpoints.map { it.role })
        // A launch is a place, the others have a size.
        assertEquals(task.turnpoints[0].roleInWords, task.turnpoints[0].detail)
        assertTrue(task.turnpoints[1].detail, task.turnpoints[1].detail.endsWith(" · 1 km"))
        assertTrue(task.turnpoints[3].detail, task.turnpoints[3].detail.endsWith(" · 400 m"))

        val summary = TaskWords.summary(task)
        assertTrue(summary, " km along the shortest way through the edges of its cylinders" in summary)
        assertTrue(summary, summary.contains("Measured on the "))
        // The launch window, the start with its gate, and the end of the task.
        val times = TaskWords.times(task)
        assertEquals("Launch open", times.first().first)
        assertEquals("Task ends", times.last().first)
        assertTrue(times.any { it.second.startsWith("Start ") })

        // It is the same task after the app was closed, and it goes out as a file.
        assertEquals(task.turnpoints.map { it.name }, checkNotNull(logbook.makeTasks().task).turnpoints.map { it.name })
        assertTrue(checkNotNull(tasks.file).name.endsWith(TaskModel.FILE_ENDING))

        tasks.forget()
        assertNull(tasks.task)
        assertNull(logbook.makeTasks().task)
    }

    @Test
    fun whatIsNoTaskIsSaidAndTheTaskThatWasThereStays() = runBlocking {
        val tasks = logbook().makeTasks()
        assertTrue(tasks.keep(sample))

        assertFalse(tasks.keep("hello"))
        assertTrue(checkNotNull(tasks.problem).isNotBlank())
        assertNotNull(tasks.task)

        // A file that cannot be read, and one too large to be a task.
        assertFalse(tasks.keep(FlightFileSource("task.xctsk") { throw IOException("gone") }))
        assertTrue(checkNotNull(tasks.problem).startsWith("The file could not be read."))
        assertFalse(tasks.keep(FlightFileSource("huge.xctsk") { "x".repeat(300 * 1024) }))
        assertEquals("This file is too large to be a task.", tasks.problem)
        assertNotNull(tasks.task)

        assertTrue(tasks.keep(FlightFileSource("task.xctsk") { sample }))
        assertNull(tasks.problem)
    }

    @Test
    fun aRouteIsMadeOfThePilotsWaypointsInTheOrderGiven() {
        val logbook = logbook()
        val waypoints = logbook.makeWaypoints()
        waypoints.importText(LaunchOptions.SAMPLE_WAYPOINT_FILE)
        val tasks = logbook.makeTasks()
        val (field, _, turnpoint) = waypoints.all

        // One waypoint is no route.
        assertFalse(tasks.keepRoute(listOf(field)))
        assertNotNull(tasks.problem)
        assertNull(tasks.task)

        // Out to a place and back is a route too.
        assertTrue(tasks.keepRoute(listOf(field, turnpoint, field)))
        val route = checkNotNull(tasks.task)
        assertTrue(route.isRoute)
        assertEquals(listOf(field.name, turnpoint.name, field.name), route.turnpoints.map { it.title })
        assertTrue(route.inALine, route.inALine.startsWith("3 waypoints · "))
        assertTrue(TaskWords.summary(route).contains("through its waypoints"))
    }

    @Test
    fun lengthsAreWrittenTheWayTaskBoardsWriteThem() {
        assertEquals("400 m", TaskWords.length(400.0))
        assertEquals("5 km", TaskWords.length(5000.0))
        assertEquals("2.5 km", TaskWords.length(2500.0))
    }

    /** Sensors that report nothing: a replay feeds the session. */
    private class NoSensors : FlightSensors {
        override fun start(session: FlightSession, onProblem: (String) -> Unit) = Unit

        override fun stop() = Unit
    }

    /** Remembers what it was asked to say. */
    private class Spoken : Announcer {
        val said: MutableList<String> = Collections.synchronizedList(mutableListOf<String>())

        override fun say(text: String) {
            said += text
        }

        override fun stop() = Unit
    }

    @Test
    fun theCockpitFliesTheKeptTaskFromItsStartToGoalAndSaysWhatHappens() {
        val logbook = logbook()
        val tasks = logbook.makeTasks()
        assertTrue(tasks.keep(sample))
        val voice = Spoken()
        val fly = FlySession(logbook.folder, MemoryPreferences(), NoSensors(), SilentTone(), scope, voice)
        fly.taskFile = { tasks.file }
        assertNull(fly.task)

        // Five hundred times faster: the half hour of the flight in a few seconds.
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        val seen = mutableListOf(checkNotNull(fly.task))
        val until = System.currentTimeMillis() + 20_000
        while (fly.task?.phase != TaskPhase.IN_GOAL && System.currentTimeMillis() < until) {
            Thread.sleep(50)
            fly.task?.let { if (it.phase != seen.last().phase) seen += it }
        }
        val end = checkNotNull(fly.task)
        if (end.phase != seen.last().phase) seen += end

        // Before the start the line counts down or says that the start is
        // open, and names the edge to cross.
        val before = seen.first()
        assertEquals(TaskPhase.BEFORE_START, before.phase)
        val headline = TaskStripWords.headline(before)
        assertTrue(headline, headline.startsWith("Start in ") || headline == "Start open")
        assertTrue(TaskStripWords.detail(before), TaskStripWords.detail(before).startsWith("Edge "))
        assertTrue(TaskStripWords.goalDetail(before).startsWith("to goal"))
        assertTrue(TaskStripWords.spoken(before).endsWith(" kilometres to goal"))

        // On task it runs the clock and says which turnpoint is next.
        val onTask = seen.first { it.phase == TaskPhase.ON_TASK }
        assertTrue(TaskStripWords.headline(onTask).startsWith("On task"))
        assertTrue(TaskStripWords.detail(onTask), " of 4" in TaskStripWords.detail(onTask))

        // The flight reaches the goal, and the start, the turnpoint and
        // the goal are ticked.
        assertEquals(TaskPhase.IN_GOAL, end.phase)
        assertEquals("In goal", TaskStripWords.headline(end))
        assertEquals(4, end.reached.size)
        assertTrue(end.reached.toString(), end.reached.drop(1).all { it })
        assertFalse(TaskStripWords.spoken(end).contains("to goal"))
        // What happened was said once each: the start, the turnpoint, the goal.
        assertTrue(voice.said.toString(), voice.said.size >= 3)
        assertEquals(voice.said.size, voice.said.toSet().size)

        // The map draws every cylinder, and one of them as the next.
        val task = checkNotNull(tasks.task)
        val rings = CockpitMap.taskFeatures(task.turnpoints, next = 3u)
        assertEquals(task.turnpoints.count { it.outline.size >= 2 }, rings.size)
        assertEquals(1, rings.count { it.getNumberProperty(CockpitMap.NEXT).toInt() == 1 })

        // Stepping back takes the goal away again; a place the pilot
        // chooses is remembered by name, and forgotten with the session.
        fly.stepTask(forward = false)
        assertNotNull(checkNotNull(fly.task).next)
        assertNull(fly.chosenPlace)
        val waypoints = logbook.makeWaypoints().also { it.importText(LaunchOptions.SAMPLE_WAYPOINT_FILE) }
        fly.goTo(waypoints.all.first().place)
        assertEquals(waypoints.all.first().name, fly.chosenPlace)
        fly.finish()
        assertNull(fly.task)
        assertNull(fly.chosenPlace)
    }

    @Test
    fun withoutATaskTheCockpitFliesNone() {
        val logbook = logbook()
        val fly = FlySession(logbook.folder, MemoryPreferences(), NoSensors(), SilentTone(), scope)
        fly.startReplay(TestSupport.sampleFlight, speed = 500.0)
        Thread.sleep(300)
        assertNull(fly.task)
        fly.stepTask(forward = true)
        assertNull(fly.task)
        fly.finish()
    }

    @Test
    fun theSketchKeepsTheTasksShapeAndFitsItIntoItsFrame() {
        val tasks = logbook().makeTasks()
        assertTrue(tasks.keep(sample))
        val task = checkNotNull(tasks.task)
        val frame = SketchFrame(task, wide = 300.0, high = 200.0, margin = 8.0)

        val places = task.turnpoints.flatMap { it.outline } + task.way
        for (place in places) {
            val (x, y) = frame.point(place)
            assertTrue("$x", x in 7.9..292.1)
            assertTrue("$y", y in 7.9..192.1)
        }
        // North is up and east is right.
        val launch = task.turnpoints.first().position
        val goal = task.turnpoints.last().position
        assertTrue(frame.point(goal).first < frame.point(launch).first)
        // A cylinder is drawn round: as wide as it is high.
        val ring = task.turnpoints[1].outline.map(frame::point)
        val width = ring.maxOf { it.first } - ring.minOf { it.first }
        val height = ring.maxOf { it.second } - ring.minOf { it.second }
        assertEquals(width, height, width * 0.02)
    }
}
