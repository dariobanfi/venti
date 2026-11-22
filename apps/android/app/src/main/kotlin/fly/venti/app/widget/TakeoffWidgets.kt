package fly.venti.app.widget

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import androidx.glance.appwidget.updateAll
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import fly.venti.app.Session
import fly.venti.app.VentiApplication
import fly.venti.app.sites.KeptSitesModel
import fly.venti.app.today.TodayModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.ZoneId
import java.util.concurrent.TimeUnit

/**
 * The home-screen widgets, "the next days at my take-off", as the rest of
 * the app sees them: what Today's model reads of the pilot's own take-offs
 * is kept for them, and they are drawn again when that or the pilot's
 * take-offs change.
 *
 * A widget draws a snapshot and asks no server. What a forecast says about
 * a take-off is the core's, read through Today's model, here as on Today.
 */
object TakeoffWidgets {
    /** Counts up whenever what a widget shows may have changed: a widget that is being drawn reads again. */
    val changes = MutableStateFlow(0)

    fun store(context: Context): WidgetStore = StoredWidgetStore(context)

    /** The widgets on the home screen, by Android's numbers for them. */
    fun onHomeScreen(context: Context): List<Int> =
        AppWidgetManager.getInstance(context)
            ?.getAppWidgetIds(ComponentName(context, TakeoffWidgetReceiver::class.java))
            ?.toList()
            .orEmpty()

    /**
     * Ties the widgets to the app's session: an outlook Today's model has
     * read of one of the pilot's own take-offs is kept and drawn, and a
     * take-off kept or forgotten draws the widgets again and has the
     * forecast of a new one read.
     */
    fun follow(context: Context, today: TodayModel, kept: KeptSitesModel, scope: CoroutineScope) {
        val app = context.applicationContext
        val store = store(app)
        today.onOutlook = { site, outlook ->
            scope.launch {
                if (kept.isKept(site.id)) {
                    store.keep(TakeoffSnapshot.of(site, outlook, System.currentTimeMillis() / 1000))
                    redraw(app)
                }
            }
        }
        val before = kept.onChanged
        kept.onChanged = {
            before()
            if (onHomeScreen(app).isNotEmpty()) {
                scope.launch { redraw(app) }
                TakeoffWidgetRefresh.now(app)
            }
        }
    }

    /** Draws every widget again from what is kept. */
    suspend fun redraw(context: Context) {
        changes.value += 1
        try {
            TakeoffWidget().updateAll(context.applicationContext)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // A launcher that cannot be reached now is drawn for at the widget's next update.
        }
    }

    /** The pilot's own take-offs as the core lists them without a place: by name. None when they cannot be read. */
    suspend fun takeoffs(kept: KeptSitesModel): List<WidgetTakeoff> = try {
        kept.todayTakeoffs(place = null, near = null).takeoffs
            .filter { it.isKept }
            .map { WidgetTakeoff(it.site.id, it.site.name) }
    } catch (error: CancellationException) {
        throw error
    } catch (_: Exception) {
        emptyList()
    }

    /** What a widget shows now: its take-off among the pilot's own, and that take-off's snapshot. */
    suspend fun content(context: Context, widgetId: Int): WidgetContent = withContext(Dispatchers.IO) {
        val session = (context.applicationContext as? VentiApplication)?.session as? Session.Ready
        val store = store(context)
        val takeoff = WidgetRules.takeoff(store.chosen(widgetId), session?.let { takeoffs(it.keptSites) }.orEmpty())
        WidgetRules.content(
            takeoff,
            takeoff?.let { store.snapshot(it.id) },
            nowUnixS = System.currentTimeMillis() / 1000,
            zone = ZoneId.systemDefault(),
        )
    }

    /**
     * Brings the take-offs of the widgets on the home screen up to date,
     * through Today's model: each is asked of Venti anew, and what the
     * model reads is kept and drawn as above. Without an answer what is
     * kept stays, and says how old it is.
     */
    suspend fun refresh(context: Context, session: Session.Ready) {
        val store = store(context)
        val own = session.keptSites.todayTakeoffs(place = null, near = null).takeoffs.filter { it.isKept }
        val listed = own.map { WidgetTakeoff(it.site.id, it.site.name) }
        val wanted = onHomeScreen(context).mapNotNull { WidgetRules.takeoff(store.chosen(it), listed)?.id }.toSet()
        for (takeoff in own) {
            if (takeoff.site.id in wanted) session.today.freshOutlook(takeoff.site)
        }
    }
}

/**
 * Brings the widgets up to date a few times a day, with a network: Android
 * decides the moment. It runs only while a widget is on the home screen.
 */
class TakeoffWidgetRefresh(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val app = applicationContext
        if (TakeoffWidgets.onHomeScreen(app).isEmpty()) return Result.success()
        val session = (app as VentiApplication).session as? Session.Ready ?: return Result.success()
        try {
            TakeoffWidgets.refresh(app, session)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // What is kept stays, with its time.
        }
        // Drawn again whatever came back: a day may have passed meanwhile.
        TakeoffWidgets.redraw(app)
        return Result.success()
    }

    companion object {
        private const val NAME = "takeoff-widget"
        private const val NAME_NOW = "takeoff-widget-now"
        private const val HOURS_BETWEEN = 6L

        private val withNetwork = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

        /** Asks Android to run it from now on; one already asked for stays as it is. */
        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<TakeoffWidgetRefresh>(HOURS_BETWEEN, TimeUnit.HOURS)
                .setConstraints(withNetwork)
                .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        /** Once, as soon as there is a network: after a take-off was chosen or kept. */
        fun now(context: Context) {
            val request = OneTimeWorkRequestBuilder<TakeoffWidgetRefresh>().setConstraints(withNetwork).build()
            WorkManager.getInstance(context).enqueueUniqueWork(NAME_NOW, ExistingWorkPolicy.REPLACE, request)
        }

        /** The last widget is gone. */
        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
            WorkManager.getInstance(context).cancelUniqueWork(NAME_NOW)
        }
    }
}
