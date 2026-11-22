package fly.venti.app.fly

import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.settings.SunWords
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** The cockpit's hold on the screen's brightness: kept when it turns it up, and put back. */
class CockpitBrightnessTest {
    /** A screen that remembers what it was asked for. */
    private class Screen(override var level: Float) : ScreenBrightness

    @Test
    fun theBrightnessIsKeptTurnedUpAndPutBack() {
        // Less than nothing is "as the phone is set", which is what a window starts with.
        for (before in listOf(-1f, 0.3f, 1f)) {
            val screen = Screen(before)
            val brightness = CockpitBrightness(screen)
            brightness.turnUp()
            assertEquals(CockpitBrightness.FULL, screen.level, 0f)
            brightness.putBack()
            assertEquals(before, screen.level, 0f)
        }
    }

    @Test
    fun turnedUpTwiceItStillGoesBackToWhatItWasAtFirst() {
        val screen = Screen(0.4f)
        val brightness = CockpitBrightness(screen)
        brightness.turnUp()
        brightness.turnUp()
        assertEquals(1f, screen.level, 0f)
        brightness.putBack()
        assertEquals(0.4f, screen.level, 0f)
    }

    @Test
    fun puttingBackWhatWasNotTurnedUpLeavesTheScreenAlone() {
        val screen = Screen(0.6f)
        val brightness = CockpitBrightness(screen)
        brightness.putBack()
        assertEquals(0.6f, screen.level, 0f)
        // And once put back, a second time changes nothing the pilot set since.
        brightness.turnUp()
        brightness.putBack()
        screen.level = 0.2f
        brightness.putBack()
        assertEquals(0.2f, screen.level, 0f)
    }

    @Test
    fun theCockpitTurnsTheScreenUpUntilThePilotSaysOtherwise() {
        // As on the iPhone: the setting is whether the brightness is left alone, and it is not.
        assertFalse(MemoryPreferences().cockpitKeepsBrightness)
        assertEquals("Full brightness in the cockpit", SunWords.FULL_BRIGHTNESS)
    }
}
