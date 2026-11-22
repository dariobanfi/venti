package fly.venti.app.sites

import fly.venti.core.TargetPlace
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.pilotMessage
import fly.venti.core.CockpitTakeoffs
import fly.venti.core.Coordinate
import fly.venti.core.Core
import fly.venti.core.Credit
import fly.venti.core.KeptSites
import fly.venti.core.SiteView
import fly.venti.core.SitesMap
import fly.venti.core.TodayTakeoffs
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The pilot's own take-offs as the screens see them: the sites the pilot
 * has marked as theirs.
 *
 * The core keeps them on the phone, with their source's credit, and says
 * which of them Today lists; this class asks it, away from the screen's
 * thread, and counts the changes so that a screen knows when to ask again.
 * Signed in, the list is on the pilot's account too: the account's model
 * runs the core's sync after each change ([onChanged]) and says when a
 * sync is through ([synced]). Signed out, nothing of it goes to a server.
 */
class KeptSitesModel(
    private val core: Core,
    /** Where the core reads and writes its files, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** How often a site was kept or forgotten since the app started: what lists them asks again when it differs. */
    var changes: Int by mutableIntStateOf(0)
        private set

    /** Why the last keeping or forgetting did not work, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    /** Called after a site was kept or forgotten: the account then brings its list in step. */
    var onChanged: () -> Unit = {}

    /**
     * A sync with the account is through: what lists the sites reads them
     * again, for what the account brought or took, and for the core's note
     * on where the list is kept.
     */
    fun synced() {
        changes += 1
    }

    /**
     * The credit and the date a site was kept with before it was forgotten
     * in this run of the app: a site kept again from a page that has
     * neither, without a network, keeps them.
     */
    private val forgotten = mutableMapOf<String, Pair<List<Credit>, Long?>>()

    /**
     * The take-offs for the cockpit's map around the glider, as the core
     * picks them: the pilot's own and those of `near`, an answer for the
     * area around the glider that the app holds. `null` when the core
     * could not read its files.
     */
    suspend fun cockpitTakeoffs(glider: Coordinate, near: SitesMap?): CockpitTakeoffs? = withContext(background) {
        runCatching { core.cockpitTakeoffs(glider, near) }.getOrNull()
    }

    /**
     * The landings the cockpit keeps a standing reading of, nearest first,
     * as the core picks them: among the sites of `near`, the pilot's own
     * kept sites and the pilot's landing waypoints. None when the core
     * could not read its files.
     */
    suspend fun cockpitLandings(glider: Coordinate, near: SitesMap?): List<TargetPlace> = withContext(background) {
        runCatching { core.cockpitLandings(glider, near) }.getOrDefault(emptyList())
    }

    /** Whether a site is one of the pilot's own. */
    suspend fun isKept(id: String): Boolean = withContext(background) {
        try {
            core.isSiteKept(id)
        } catch (_: Exception) {
            false
        }
    }

    /**
     * Marks a site as one of the pilot's own, with the credits of the
     * answer it came from and the date of that answer's data. Says whether
     * it is kept now.
     */
    suspend fun keep(site: SiteView, credits: List<Credit>, asOfUnixS: Long?): Boolean {
        val before = forgotten[site.id]
        val done = change { core.keepSite(site, credits + before?.first.orEmpty(), asOfUnixS ?: before?.second) }
        if (done) forgotten.remove(site.id)
        return done
    }

    /** Unmarks a site. Says whether that worked. */
    suspend fun forget(id: String): Boolean {
        val had = all(null)
        val kept = had?.sites?.firstOrNull { it.site.id == id }
        val done = change { core.forgetSite(id) }
        if (done && had != null && kept != null) {
            forgotten[id] = had.credits.filter { it.source == kept.site.source } to kept.asOfUnixS
        }
        return done
    }

    /**
     * The pilot's own sites, from the phone alone: with a place the nearest
     * first, each with its distance, without one by name. `null` when they
     * could not be read; [problem] then says why.
     */
    suspend fun all(from: Coordinate?): KeptSites? {
        val read = withContext(background) { runCatching { core.keptSites(from) } }
        read.exceptionOrNull()?.let { problem = it.pilotMessage }
        return read.getOrNull()
    }

    /**
     * The take-offs Today lists: the pilot's own first, then those around
     * the place in the server's answer; without an answer, the pilot's own.
     */
    suspend fun todayTakeoffs(place: Coordinate?, near: SitesMap?): TodayTakeoffs = withContext(background) {
        core.todayTakeoffs(place, near)
    }

    private suspend fun change(act: () -> Unit): Boolean {
        val failure = withContext(background) {
            try {
                act()
                null
            } catch (error: Exception) {
                error.pilotMessage
            }
        }
        problem = failure
        // Counted also when it failed: a screen then reads what is true.
        changes += 1
        if (failure == null) onChanged()
        return failure == null
    }
}
