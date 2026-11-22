package fly.venti.app.widget

import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.RemoteViews
import android.widget.TextView
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.glance.appwidget.ExperimentalGlanceRemoteViewsApi
import androidx.glance.appwidget.GlanceRemoteViews
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.TestDevice
import fly.venti.app.design.Format
import fly.venti.app.today.TodayWords
import fly.venti.core.Flyability
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * The home-screen widget "the next days at my take-off", as a launcher
 * would draw it: the widget's own views, made by Glance as for a launcher,
 * laid on an empty screen in both of its sizes, since a test cannot place
 * a widget on the launcher. Each state it has, in the light and the dark
 * appearance, with a picture of each.
 *
 * The take-off and its days are made up here, in the words the core uses;
 * what is shown of them at a moment is the widget's own rule
 * (`WidgetRules.content`).
 */
@OptIn(ExperimentalGlanceRemoteViewsApi::class)
@RunWith(AndroidJUnit4::class)
class TakeoffWidgetTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val zone: ZoneId = ZoneId.systemDefault()

    /** A moment of a day of November 2026 on the device's clock, which is the clock the widget's times are written by. */
    private fun at(day: Int, time: String = "00:00"): Long =
        LocalDate.of(2026, 11, day).atTime(LocalTime.parse(time)).atZone(zone).toEpochSecond()

    private val takeoff = WidgetTakeoff("made-up", "Wallberg (made up)")

    private val read = at(22, "14:05")

    private val snapshot = TakeoffSnapshot(
        siteId = takeoff.id,
        siteName = takeoff.name,
        days = listOf(
            SnapshotDay(at(22), "Today", Flyability.FLYABLE, "Flyable", "Good from 11:00 to 15:00"),
            SnapshotDay(at(23), "Tomorrow", Flyability.MARGINAL, "Marginal", "Marginal"),
            SnapshotDay(at(24), "Tue", Flyability.NOT_FLYABLE, "Not flyable", "Not flyable"),
            SnapshotDay(at(25), "Wed", Flyability.FLYABLE, "Flyable", "Good until 14:00"),
        ),
        takenUnixS = read,
    )

    /** What the two sizes say, the wide one first. */
    private class Said(val wide: List<String>, val small: List<String>)

    @Test
    fun theWidgetShowsTheNextDaysAtATakeoff() = look("light", dark = false)

    @Test
    fun theWidgetInDarkAppearance() = look("dark", dark = true)

    private fun look(prefix: String, dark: Boolean) {
        TestDevice.keepSystemDialogsAway()
        val words = instrumentation.targetContext
        ActivityScenario.launch<WidgetStage>(Intent(words, WidgetStage::class.java)).use { stage ->
            // An hour after it was read: the days, in the core's words.
            show(stage, WidgetRules.content(takeoff, snapshot, at(22, "15:00"), zone), dark).let { said ->
                val updated = words.getString(R.string.takeoff_widget_updated, Format.time(read))
                for (size in listOf(said.wide, said.small)) {
                    assertEquals(takeoff.name, size.first())
                    assertEquals(updated, size.last())
                    assertTrue("$size", size.containsAll(listOf("Today", "Flyable", "Good from 11:00 to 15:00", "Tomorrow", "Marginal")))
                }
                // The wide one has the days Today offers, the small one two.
                assertEquals(WidgetRules.MOST_DAYS, said.wide.count { it in STATUS_WORDS })
                assertTrue("${said.wide}", "Tue" in said.wide && "Not flyable" in said.wide)
                assertEquals(WidgetRules.DAYS_WHEN_SMALL, said.small.count { it in STATUS_WORDS })
                TestDevice.keepScreenshot("widget-1-days-$prefix")
            }
            // The next morning: Sunday has passed, and Monday is called by its day.
            show(stage, WidgetRules.content(takeoff, snapshot, at(23, "08:00"), zone), dark).let { said ->
                val updated = words.getString(R.string.takeoff_widget_updated, Format.dayAndTime(read))
                for (size in listOf(said.wide, said.small)) {
                    assertEquals(updated, size.last())
                    assertFalse("$size", "Today" in size || "Tomorrow" in size)
                    assertTrue("$size", size.containsAll(listOf("Mon", "Marginal", "Tue", "Not flyable")))
                }
                TestDevice.keepScreenshot("widget-2-next-morning-$prefix")
            }
            // Older than a day: it says so, and no status of any day.
            show(stage, WidgetRules.content(takeoff, snapshot, at(23, "18:00"), zone), dark).let { said ->
                val stale = words.getString(R.string.takeoff_widget_stale)
                val updated = words.getString(R.string.takeoff_widget_updated, Format.dayAndTime(read))
                assertEquals(listOf(takeoff.name, stale, updated), said.wide)
                assertEquals(said.wide, said.small)
                TestDevice.keepScreenshot("widget-3-older-than-a-day-$prefix")
            }
            // A take-off whose forecast has not been read yet.
            show(stage, WidgetRules.content(takeoff, null, at(22, "15:00"), zone), dark).let { said ->
                assertEquals(listOf(takeoff.name, TodayWords.READING), said.wide)
                assertEquals(said.wide, said.small)
                TestDevice.keepScreenshot("widget-4-reading-$prefix")
            }
            // The pilot keeps no take-off.
            show(stage, WidgetRules.content(null, null, at(22, "15:00"), zone), dark).let { said ->
                assertEquals(listOf(words.getString(R.string.takeoff_widget_no_takeoff)), said.wide)
                assertEquals(said.wide, said.small)
                TestDevice.keepScreenshot("widget-5-no-takeoff-$prefix")
            }
        }
    }

    /**
     * Lays the widget on the stage for what it has to show, wide and
     * small one under the other, in an appearance, and reads back what
     * each says.
     */
    private fun show(stage: ActivityScenario<WidgetStage>, content: WidgetContent, dark: Boolean): Said {
        lateinit var themed: Context
        stage.onActivity { activity ->
            val appearance = Configuration(activity.resources.configuration).apply {
                val night = if (dark) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
                uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or night
            }
            themed = activity.createConfigurationContext(appearance)
        }
        // The views as Glance makes them for a launcher that gives the widget one of its two sizes.
        val sizes = listOf(TakeoffWidget.WIDE to ON_STAGE_WIDE, TakeoffWidget.SMALL to ON_STAGE_SMALL)
        val made: List<RemoteViews> = runBlocking {
            sizes.map { (size, _) -> GlanceRemoteViews().compose(themed, size) { TakeoffWidgetFace(content) }.remoteViews }
        }
        val said = mutableListOf<List<String>>()
        stage.onActivity { activity ->
            fun px(dp: Dp): Int =
                TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, dp.value, activity.resources.displayMetrics).toInt()
            activity.stage.removeAllViews()
            activity.stage.setBackgroundColor(themed.getColor(R.color.venti_bg))
            made.zip(sizes).forEach { (views, size) ->
                val frame = FrameLayout(themed)
                val room = LinearLayout.LayoutParams(px(size.second.width), px(size.second.height))
                room.setMargins(px(VentiSpace.s4), px(VentiSpace.s4), px(VentiSpace.s4), 0)
                activity.stage.addView(frame, room)
                frame.addView(views.apply(themed, frame))
                said += texts(frame)
            }
        }
        // The picture is of the screen, so the stage has to have drawn
        // what was just laid on it: two frames are waited for, the second
        // asked for when the first has reached the screen's own drawing.
        repeat(2) {
            val drawn = CountDownLatch(1)
            stage.onActivity { activity ->
                activity.stage.viewTreeObserver.registerFrameCommitCallback { drawn.countDown() }
                activity.stage.invalidate()
            }
            assertTrue("the stage drew no frame", drawn.await(10, TimeUnit.SECONDS))
        }
        instrumentation.waitForIdleSync()
        return Said(wide = said[0], small = said[1])
    }

    /** Every text of a view and what is inside it, from the top down. */
    private fun texts(view: View): List<String> = when (view) {
        is TextView -> listOf(view.text.toString()).filter { it.isNotEmpty() }
        is ViewGroup -> (0 until view.childCount).flatMap { texts(view.getChildAt(it)) }
        else -> emptyList()
    }

    private companion object {
        val STATUS_WORDS = setOf("Flyable", "Marginal", "Not flyable")

        /**
         * The room the widget is given on the stage, about what a launcher
         * with four columns gives two cells by two and four by two on a
         * phone 411 dp wide. A launcher's own measures differ.
         */
        val ON_STAGE_WIDE = DpSize(364.dp, 190.dp)
        val ON_STAGE_SMALL = DpSize(172.dp, 190.dp)
    }
}
