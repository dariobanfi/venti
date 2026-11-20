package fly.venti.app.community

import fly.venti.core.ReportReason
import fly.venti.core.ReportSubject
import fly.venti.core.reportForm
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** What the form for a report says and when it offers nothing to fill in. */
class ReportSheetTest {
    @Test
    fun aReportNeedsAnAccountOnTheServerTheThingIsOn() {
        val server = "https://venti.example.org"
        // Signed out, or with no account at all.
        assertEquals(ReportWords.NO_ACCOUNT, ReportWords.obstacle(isSignedIn = false, mine = server, shown = server))
        assertEquals(ReportWords.NO_ACCOUNT, ReportWords.obstacle(isSignedIn = false, mine = null, shown = server))
        assertEquals(ReportWords.NO_ACCOUNT, ReportWords.obstacle(isSignedIn = true, mine = null, shown = server))
        // Signed in on another server than the one this is on, or with
        // no server set to have seen it on.
        assertEquals(
            ReportWords.OTHER_SERVER,
            ReportWords.obstacle(isSignedIn = true, mine = server, shown = "https://other.example.org"),
        )
        assertEquals(ReportWords.OTHER_SERVER, ReportWords.obstacle(isSignedIn = true, mine = server, shown = null))
        // On the server the thing is on, nothing stands in the way; and
        // two addresses of one server are one server.
        assertNull(ReportWords.obstacle(isSignedIn = true, mine = server, shown = server))
        assertNull(ReportWords.obstacle(isSignedIn = true, mine = server, shown = "https://Venti.example.org/"))
        assertTrue(ReportWords.same("https://venti.example.org", "https://Venti.example.org/"))
        assertFalse(ReportWords.same("https://venti.example.org", "https://other.example.org"))
        // Where to sign in is said as the settings call it.
        assertTrue(ReportWords.NO_ACCOUNT.endsWith("Sign in under Settings, Account."))
    }

    @Test
    fun theFormListsTheCoresReasonsAndCountsTheWordsOnlyWhenTheyAreTooMany() {
        val form = reportForm()
        assertEquals(6, form.reasons.map { it.reason }.distinct().size)
        assertEquals("Something else", form.reasons.single { it.reason == ReportReason.OTHER }.title)
        assertTrue(form.reasons.all { it.title.isNotBlank() && it.explanation.endsWith(".") })
        // A report is not anonymous, and the form has the words that say so.
        assertTrue(form.notice.contains("sees that it came from you"))

        // Over the field for the pilot's own words: with "Something else"
        // they are what the report is.
        assertEquals("Anything to add?", ReportWords.overTheText(null))
        assertEquals("Anything to add?", ReportWords.overTheText(ReportReason.SPAM))
        assertEquals("What is it?", ReportWords.overTheText(ReportReason.OTHER))

        // The count is said once the text is too long, in characters as
        // the server counts them and not in the units a phone stores.
        val most = form.textAtMost
        val paraglider = "🪂"
        assertNull(ReportWords.tooLong("", most))
        assertNull(ReportWords.tooLong("a".repeat(most.toInt()), most))
        assertNull(ReportWords.tooLong(paraglider.repeat(most.toInt()), most))
        assertEquals("${most + 1u} of $most characters", ReportWords.tooLong("a".repeat(most.toInt() + 1), most))
        assertEquals("${most + 1u} of $most characters", ReportWords.tooLong(paraglider.repeat(most.toInt() + 1), most))
    }

    @Test
    fun whatIsReportedGivesItsFormItsName() {
        assertEquals(Reported(ReportSubject.Flight("fbyw03atd9qdt5bfe177ra1k94"), "Report this flight"), Reported.flight("fbyw03atd9qdt5bfe177ra1k94"))
        assertEquals(Reported(ReportSubject.Pilot("mara"), "Report this pilot"), Reported.pilot("mara"))
        assertEquals(
            Reported(ReportSubject.LiveTrack("11111111111111111111111111"), "Report this track"),
            Reported.liveTrack("11111111111111111111111111"),
        )
    }
}
