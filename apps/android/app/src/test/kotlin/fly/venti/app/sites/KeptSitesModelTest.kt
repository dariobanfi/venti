package fly.venti.app.sites

import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.core.Coordinate
import fly.venti.core.SiteKind
import fly.venti.core.sitesMap
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.Locale

/**
 * The pilot's own take-offs against the real core, which keeps them in a
 * folder of the test's own, with the sites of the answer the real server's
 * tests keep for `/v1/sites`: five sites east of the Tegernsee.
 */
class KeptSitesModelTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    private fun model(): KeptSitesModel =
        LogbookModel.open(TestSupport.newFolder("kept-sites"), TestSupport.files).makeKeptSites()

    private val map = sitesMap(
        47.6, 11.7, 47.8, 12.0, 200.toUShort(), TestSupport.recordedAnswers.resolve("sites.json").readBytes(),
    )
    private val takeoff = map.sites.first { it.isNamed && SiteKind.TAKEOFF in it.uses }
    private val landing = map.sites.first { it.isNamed && SiteKind.TAKEOFF !in it.uses }

    @Test
    fun aSiteIsKeptWithItsCreditAndItsDateAndForgottenAgain() = runBlocking {
        val kept = model()
        assertFalse(kept.isKept(takeoff.id))
        assertEquals(emptyList<String>(), kept.all(null)?.sites?.map { it.site.id })

        assertTrue(kept.keep(takeoff, map.credits, map.asOfUnixS))
        assertEquals(1, kept.changes)
        assertTrue(kept.isKept(takeoff.id))
        assertNull(kept.problem)
        // Any site can be kept, a landing too.
        assertTrue(kept.keep(landing, map.credits, map.asOfUnixS))

        // Without a place by name, with the core's note and the source's credit.
        val all = checkNotNull(kept.all(null))
        assertEquals(listOf(takeoff.name, landing.name), all.sites.map { it.site.name })
        assertEquals(map.asOfUnixS, all.sites.first().asOfUnixS)
        assertTrue(all.note, all.note.startsWith("Kept on this device only"))
        assertTrue(all.credits.any { "OpenStreetMap" in it.text })
        // What the source says of the site is kept with it.
        assertEquals(takeoff.website, all.sites.first().site.website)
        assertEquals(takeoff.feeInWords, all.sites.first().site.feeInWords)

        // From a place the nearest first, each with its distance and direction.
        val fromTheLanding = checkNotNull(kept.all(landing.position))
        assertEquals(listOf(landing.id, takeoff.id), fromTheLanding.sites.map { it.site.id })
        assertTrue(checkNotNull(fromTheLanding.sites.last().distanceM) > 1000)

        assertTrue(kept.forget(landing.id))
        assertFalse(kept.isKept(landing.id))
        assertEquals(listOf(takeoff.id), kept.all(null)?.sites?.map { it.site.id })
        assertEquals(3, kept.changes)
    }

    @Test
    fun aSiteForgottenAndKeptAgainWithoutAnAnswerKeepsItsCreditAndItsDate() = runBlocking {
        val kept = model()
        kept.keep(takeoff, map.credits, map.asOfUnixS)
        kept.forget(takeoff.id)
        assertEquals(emptyList<String>(), kept.all(null)?.credits?.map { it.text })

        // The page it is kept from again has no answer of a server at hand.
        assertTrue(kept.keep(takeoff, emptyList(), null))
        val all = checkNotNull(kept.all(null))
        assertEquals(map.asOfUnixS, all.sites.single().asOfUnixS)
        assertTrue(all.credits.any { "OpenStreetMap" in it.text })
    }

    @Test
    fun aKeptSiteSaysHowFarItIsWhatItIsAndHowOldItsRecordIs() = runBlocking {
        val kept = model()
        kept.keep(takeoff, map.credits, map.asOfUnixS)

        val byName = checkNotNull(kept.all(null)).sites.single()
        val detail = KeptSitesWords.detail(byName)
        assertTrue(detail, detail.startsWith(SiteWords.detail(takeoff)))
        assertTrue(detail, "\nAs of " in detail)

        // From a place due south of it, the distance and the direction come first.
        val south = Coordinate(lat = takeoff.position.lat - 0.1, lon = takeoff.position.lon)
        val near = checkNotNull(kept.all(south)).sites.single()
        assertTrue(KeptSitesWords.detail(near), KeptSitesWords.detail(near).startsWith("11.1 km N · "))

        val under = KeptSitesWords.under(checkNotNull(kept.all(null)))
        assertTrue(under, under.startsWith("Kept on this device only"))
        assertTrue(under, "© OpenStreetMap contributors" in under)
    }

    @Test
    fun whatASourceSaysBeyondTheNameIsShownOnlyWhereItSaysIt() {
        // The recorded take-off asks a fee and names a web page; its landing names the page alone.
        assertEquals(listOf("A fee is asked for using this site."), SiteWords.said(takeoff))
        assertEquals("https://www.dhv.de/", SiteWords.website(takeoff))
        assertEquals(emptyList<String>(), SiteWords.said(landing))
        // A ramp without a name has its mapper's words, and nothing else.
        val ramp = map.sites.first { !it.isNamed && SiteKind.TAKEOFF in it.uses }
        assertEquals("Startrampe (vermutlich für Drachenflieger)", ramp.description)
        assertNull(SiteWords.website(ramp))
        // Only a web address is opened.
        assertNull(SiteWords.website(takeoff.copy(website = "javascript:alert(1)")))
        assertNull(SiteWords.website(takeoff.copy(website = "file:///etc/hosts")))
        // The credit under a page comes from the map, else from what the page was given.
        val known = SiteSources(map.credits, map.asOfUnixS)
        val footnote = SiteWords.footnote(takeoff, known)
        assertTrue(footnote, "© OpenStreetMap contributors" in footnote)
        assertTrue(footnote, "As of " in footnote)
    }
}
