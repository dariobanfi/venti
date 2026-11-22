package fly.venti.app.widget

import android.content.Context
import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import app.venti.design.VentiColors
import app.venti.design.VentiDark
import app.venti.design.VentiLight
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import app.venti.design.VentiTypeStyle
import fly.venti.app.MainActivity
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.today.TodayWords
import fly.venti.core.Flyability

/**
 * The home-screen widget "the next days at my take-off": the take-off's
 * name, for today and the next days the day's name and its status as Today
 * shows it, and when that was read. It draws what [TakeoffWidgets] keeps;
 * a tap opens the app on Today.
 */
class TakeoffWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Responsive(setOf(SMALL, WIDE))

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val widgetId = GlanceAppWidgetManager(context).getAppWidgetId(id)
        val first = TakeoffWidgets.content(context, widgetId)
        provideContent {
            // Read again whenever the app says that what is kept has changed.
            val changes by TakeoffWidgets.changes.collectAsState()
            val content by produceState(first, changes) { value = TakeoffWidgets.content(context, widgetId) }
            TakeoffWidgetFace(content)
        }
    }

    companion object {
        /**
         * The two sizes the widget is laid out for, as Android counts the
         * room of home-screen cells (70 dp for each, less 30): two by two,
         * and four by two. These are the launcher's measures, not the
         * design language's.
         */
        val SMALL = DpSize(110.dp, 110.dp)
        val WIDE = DpSize(250.dp, 110.dp)

        /** The extra of the intent a tap on the widget opens the app with: Today comes to the front. */
        const val OPENS_TODAY = "opens-today"
    }
}

/** Android's way to the widget: it asks for a drawing, and says when widgets come and go. */
class TakeoffWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TakeoffWidget()

    override fun onEnabled(context: Context) {
        super.onEnabled(context)
        TakeoffWidgetRefresh.schedule(context)
    }

    override fun onDisabled(context: Context) {
        super.onDisabled(context)
        TakeoffWidgetRefresh.cancel(context)
    }

    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        super.onDeleted(context, appWidgetIds)
        val store = TakeoffWidgets.store(context)
        appWidgetIds.forEach(store::forget)
    }
}

/** A colour of the design pack in the phone's appearance: the light theme's by day, the dark theme's by night. */
private fun tone(pick: (VentiColors) -> Color): ColorProvider = ColorProvider(day = pick(VentiLight), night = pick(VentiDark))

/**
 * One of the pack's type styles for a widget's text. A widget is drawn by
 * the launcher, which has the phone's fonts and not the app's: the sizes
 * and weights are the pack's, the family is the phone's.
 */
private fun VentiTypeStyle.widget(colour: ColorProvider, weight: Int = this.weight): TextStyle = TextStyle(
    color = colour,
    fontSize = sizePx.sp,
    fontWeight = when {
        weight >= 600 -> FontWeight.Bold
        weight >= 500 -> FontWeight.Medium
        else -> FontWeight.Normal
    },
)

private const val SEMI_BOLD = 600

/** The widget's face for what it has to show, in the size the launcher gives it. */
@Composable
fun TakeoffWidgetFace(content: WidgetContent) {
    val context = LocalContext.current
    val isWide = LocalSize.current.width >= TakeoffWidget.WIDE.width
    val open = Intent(context, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        .putExtra(TakeoffWidget.OPENS_TODAY, true)
    Column(
        modifier = GlanceModifier
            .fillMaxSize()
            .appWidgetBackground()
            .background(tone { it.surface })
            .cornerRadius(VentiRadius.m)
            .clickable(actionStartActivity(open))
            .padding(VentiSpace.s3),
    ) {
        when (content) {
            WidgetContent.NoTakeoff -> Said(context.getString(R.string.takeoff_widget_no_takeoff))
            is WidgetContent.NoForecast -> {
                Name(content.takeoff)
                Spacer(GlanceModifier.height(VentiSpace.s2))
                Said(if (content.isReading) TodayWords.READING else TodayWords.NO_FORECAST)
            }
            is WidgetContent.Stale -> {
                Name(content.takeoff)
                Spacer(GlanceModifier.height(VentiSpace.s2))
                Said(context.getString(R.string.takeoff_widget_stale))
                Spacer(GlanceModifier.defaultWeight())
                Updated(context, content.takenUnixS, isTakenToday = false)
            }
            is WidgetContent.Days -> {
                Name(content.takeoff)
                Spacer(GlanceModifier.height(VentiSpace.s2))
                val days = if (isWide) content.days else content.days.take(WidgetRules.DAYS_WHEN_SMALL)
                // The days take the room there is, and the time stands at
                // the foot whatever the launcher gives. A column of a
                // widget holds ten things at most, so each day is one.
                Column(modifier = GlanceModifier.fillMaxWidth().defaultWeight()) {
                    for (day in days) {
                        if (isWide) DayInALine(day) else DayStacked(day)
                        Spacer(GlanceModifier.height(VentiSpace.s1))
                    }
                }
                Updated(context, content.takenUnixS, content.isTakenToday)
            }
        }
    }
}

/** The take-off's name. */
@Composable
private fun Name(takeoff: String) {
    Text(takeoff, style = VentiType.label.widget(tone { it.text }, SEMI_BOLD), maxLines = 1)
}

/** A sentence in place of the days. */
@Composable
private fun Said(words: String) {
    Text(words, style = VentiType.label.widget(tone { it.textSecondary }))
}

/** When the snapshot was read: the hour on the day itself, the day too after it. */
@Composable
private fun Updated(context: Context, takenUnixS: Long, isTakenToday: Boolean) {
    val at = if (isTakenToday) Format.time(takenUnixS) else Format.dayAndTime(takenUnixS)
    Text(
        context.getString(R.string.takeoff_widget_updated, at),
        style = VentiType.caption.widget(tone { it.textSecondary }),
        // With its day the line needs a second one in the small size.
        maxLines = if (isTakenToday) 1 else 2,
    )
}

/** A day in the wide size: its name, its status, and the flying hours beside it. */
@Composable
private fun DayInALine(day: ShownDay) {
    Row(modifier = GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            day.name,
            style = VentiType.label.widget(tone { it.textSecondary }),
            maxLines = 1,
            modifier = GlanceModifier.width(VentiSpace.s8),
        )
        Status(day)
        if (day.hours.isNotEmpty()) {
            Spacer(GlanceModifier.width(VentiSpace.s2))
            Text(
                day.hours,
                style = VentiType.caption.widget(tone { it.textSecondary }),
                maxLines = 1,
                modifier = GlanceModifier.defaultWeight(),
            )
        }
    }
}

/** A day in the small size: its name, under it its status, under that the flying hours. */
@Composable
private fun DayStacked(day: ShownDay) {
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        Text(day.name, style = VentiType.caption.widget(tone { it.textSecondary }), maxLines = 1)
        Status(day)
        if (day.hours.isNotEmpty()) {
            Text(day.hours, style = VentiType.caption.widget(tone { it.textSecondary }), maxLines = 1)
        }
    }
}

/**
 * The pack's status as Today's chip has it (`design/Status.kt`): the
 * pack's icon, the core's word and the status colour on its soft ground,
 * never the colour alone.
 */
@Composable
private fun Status(day: ShownDay) {
    val (ink, ground, sign) = when (day.verdict) {
        Flyability.FLYABLE -> Triple(tone { it.good }, tone { it.goodSoft }, R.drawable.venti_good)
        Flyability.MARGINAL -> Triple(tone { it.caution }, tone { it.cautionSoft }, R.drawable.venti_caution)
        Flyability.NOT_FLYABLE -> Triple(tone { it.danger }, tone { it.dangerSoft }, R.drawable.venti_danger)
    }
    Row(
        modifier = GlanceModifier
            .background(ground)
            .cornerRadius(VentiRadius.s)
            .padding(start = VentiSpace.s1, end = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Image(
            ImageProvider(sign),
            contentDescription = null,
            colorFilter = ColorFilter.tint(ink),
            modifier = GlanceModifier.size(VentiSize.icon - VentiSpace.s2),
        )
        Spacer(GlanceModifier.width(VentiSpace.s1))
        Text(day.status, style = VentiType.label.widget(ink, SEMI_BOLD), maxLines = 1)
    }
}
