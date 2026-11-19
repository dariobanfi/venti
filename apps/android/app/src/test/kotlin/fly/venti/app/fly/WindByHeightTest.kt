package fly.venti.app.fly

import fly.venti.app.TestSupport
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import fly.venti.core.WindAtHeightView
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.Locale

/** The wind by height: what the core measured in each band, and how the list words it. */
class WindByHeightTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    @Test
    fun aBandIsWordedForTheEyeAndForAScreenReader() {
        val here = WindAtHeightView(
            floorM = 1600.0,
            ceilingM = 1800.0,
            fromDeg = 270.0,
            speedMs = 3.0,
            ageS = 200.0,
            isHere = true,
        )
        assertEquals("1,600 to 1,800 m", WindByHeightWords.heights(here))
        assertEquals("You are here · measured 3 minutes ago", WindByHeightWords.detail(here))
        assertEquals("11 km/h W", WindByHeightWords.wind(here))
        assertEquals("1,600 to 1,800 metres, where you are", WindByHeightWords.spokenHeights(here))
        assertEquals("11 kilometres per hour from W, measured 3 minutes ago", WindByHeightWords.spokenWind(here))

        val above = here.copy(floorM = 1800.0, ceilingM = 2000.0, ageS = 20.0, isHere = false)
        assertEquals("Measured just now", WindByHeightWords.detail(above))
        assertEquals("1,800 to 2,000 metres", WindByHeightWords.spokenHeights(above))
    }

    @Test
    fun theCoreKeepsTheWindOfEveryBandTheGliderCircledIn() {
        val session = FlightSession(TestSupport.newFolder("fly").path)
        val replay = Replay(TestSupport.sampleFlight)
        assertTrue(session.view(replay.now()).windsByHeight.isEmpty())

        // Ten minutes in, on the glide after the first thermal.
        replay.advance(session, 600.0)
        val bands = session.view(replay.now()).windsByHeight

        // The climb went through several bands of 200 m; the highest comes first.
        assertTrue("$bands", bands.size >= 3)
        assertEquals(200.0, bands.first().ceilingM - bands.first().floorM, 0.0)
        assertEquals(bands.sortedByDescending { it.floorM }, bands)
        assertTrue("$bands", bands.count { it.isHere } <= 1)
    }
}
