package fly.venti.app.widget

import android.content.Context
import androidx.core.content.edit

/**
 * What the home-screen widgets keep on the phone: which take-off the pilot
 * chose for each widget, and the last snapshot of each take-off.
 */
interface WidgetStore {
    /** The take-off chosen for a widget, by the site's identifier; `null` until one is chosen. */
    fun chosen(widgetId: Int): String?

    fun choose(widgetId: Int, siteId: String)

    /** A widget was taken off the home screen. */
    fun forget(widgetId: Int)

    /** The last snapshot of a take-off, if one is kept and can be read. */
    fun snapshot(siteId: String): TakeoffSnapshot?

    fun keep(snapshot: TakeoffSnapshot)
}

/** In memory, for tests. */
class MemoryWidgetStore : WidgetStore {
    private val chosen = mutableMapOf<Int, String>()

    /** The snapshots as their kept text, so that a test reads what was written. */
    val kept = mutableMapOf<String, String>()

    override fun chosen(widgetId: Int): String? = chosen[widgetId]

    override fun choose(widgetId: Int, siteId: String) {
        chosen[widgetId] = siteId
    }

    override fun forget(widgetId: Int) {
        chosen.remove(widgetId)
    }

    override fun snapshot(siteId: String): TakeoffSnapshot? = kept[siteId]?.let(TakeoffSnapshot::decoded)

    override fun keep(snapshot: TakeoffSnapshot) {
        kept[snapshot.siteId] = snapshot.encoded()
    }
}

/** In the app's preferences, in a file of the widget's own. */
class StoredWidgetStore(context: Context) : WidgetStore {
    private val stored = context.applicationContext.getSharedPreferences("venti-takeoff-widget", Context.MODE_PRIVATE)

    override fun chosen(widgetId: Int): String? = stored.getString(CHOSEN + widgetId, null)

    override fun choose(widgetId: Int, siteId: String) = stored.edit { putString(CHOSEN + widgetId, siteId) }

    override fun forget(widgetId: Int) = stored.edit { remove(CHOSEN + widgetId) }

    override fun snapshot(siteId: String): TakeoffSnapshot? =
        stored.getString(SNAPSHOT + siteId, null)?.let(TakeoffSnapshot::decoded)

    override fun keep(snapshot: TakeoffSnapshot) = stored.edit { putString(SNAPSHOT + snapshot.siteId, snapshot.encoded()) }

    private companion object {
        const val CHOSEN = "chosen-"
        const val SNAPSHOT = "snapshot-"
    }
}
