package fly.venti.app.flight

import fly.venti.app.HttpAnswer
import fly.venti.app.TestSupport
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.core.LeagueRequest
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * Sending a flight to a league, against the real core. The DHV-XC is played
 * by the example answers its specification gives, which `crates/leagues`
 * keeps: nothing is ever sent to it from a test.
 */
class LeaguesTest {
    private fun example(name: String): ByteArray =
        TestSupport.recordedAnswers.resolve("../../../leagues/tests/fixtures/$name").readBytes()

    private fun logbookWithTheSample(): Pair<LogbookModel, String> = runBlocking {
        val logbook = LogbookModel.open(TestSupport.newFolder("leagues"), TestSupport.files)
        logbook to logbook.addSample().added.single()
    }

    @Test
    fun beforeAFlightGoesThePilotIsToldUntilWhenAndWhetherItIsSigned() = runBlocking {
        val (logbook, id) = logbookWithTheSample()
        val leagues = Leagues(logbook, MemoryPreferences(), MemoryUploadPasswordStore()) { error("nothing is sent") }
        val handover = leagues.handover(id)
        // The sample was recorded by Venti, which signs nothing yet.
        assertFalse(handover.isSigned)
        assertTrue(handover.signature, handover.signature.startsWith("Venti recorded this flight and does not sign"))
        // It was flown in July: the fourteen days are over.
        assertTrue(handover.isLate)
        assertTrue(handover.deadline.contains("14 days"))
        assertTrue(handover.file.igcText.startsWith("AXVE"))
        assertTrue(handover.xcontestClaimUrl.startsWith("https://"))
    }

    @Test
    fun aFlightTheDhvxcTakesKeepsItsPasswordAndGoesToTheDhvxcOnly() = runBlocking {
        val (logbook, id) = logbookWithTheSample()
        val passwords = MemoryUploadPasswordStore()
        val sent = mutableListOf<LeagueRequest>()
        val leagues = Leagues(logbook, MemoryPreferences(), passwords) { request ->
            sent += request
            HttpAnswer(200, example("uploaded.json"))
        }
        assertFalse(leagues.canSend(" ", "secret"))
        assertFalse(leagues.canSend("pilot", ""))
        assertTrue(leagues.canSend("pilot", "secret"))

        val answer = leagues.sendToDhvxc(id, " pilot ", "made-up-upload-password", glider = "  ", publishes = false)
        assertTrue(answer.taken)
        assertEquals("The DHV-XC has the flight", answer.title)
        val request = sent.single()
        assertEquals("https://en.dhv-xc.de/api/v1/flights", request.url)
        assertEquals("application/json", request.contentType)
        assertTrue(request.body.decodeToString().contains("made-up-upload-password"))
        assertEquals("made-up-upload-password", passwords.read())
    }

    @Test
    fun aPasswordTheDhvxcRefusesIsNotKept() = runBlocking {
        val (logbook, id) = logbookWithTheSample()
        val passwords = MemoryUploadPasswordStore()
        val refusing = Leagues(logbook, MemoryPreferences(), passwords) { HttpAnswer(401, example("not_signed_in.json")) }
        val answer = refusing.sendToDhvxc(id, "pilot", "wrong", glider = "", publishes = true)
        assertFalse(answer.taken)
        assertEquals("The DHV-XC did not accept the sign-in", answer.title)
        assertNull(passwords.read())

        val unreachable = Leagues(logbook, MemoryPreferences(), passwords) { throw IOException("no network") }
        val nothing = unreachable.sendToDhvxc(id, "pilot", "secret", glider = "", publishes = false)
        assertFalse(nothing.taken)
        assertEquals(LeagueWords.NOT_REACHED, nothing.title)
        assertNull(passwords.read())
    }

    @Test
    fun thePilotIsAskedWithTheAccountAndWhatHappensThere() {
        assertEquals(
            "As pilot. It waits among your uploads there until you publish it. By the DHV-XC’s rules, " +
                "whoever uploads a flight gives the DHV the right to use its file.",
            LeagueWords.asking(" pilot ", publishes = false),
        )
        assertTrue(LeagueWords.asking("pilot", publishes = true).contains("publishes it at once"))
    }
}
