package fly.venti.app.fly

import fly.venti.app.TestSupport
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import fly.venti.core.WindAtHeightView
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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
            climbMs = null,
        )
        assertEquals("1,600 to 1,800 m", WindByHeightWords.heights(here))
        assertEquals("You are here · measured 3 minutes ago", WindByHeightWords.detail(here))
        assertEquals("11 km/h W", WindByHeightWords.wind(here))
        assertEquals("1,600 to 1,800 metres, where you are", WindByHeightWords.spokenHeights(here))
        assertEquals("11 kilometres per hour from W, measured 3 minutes ago", WindByHeightWords.spokenWind(here))

        // A band the glider circled in only briefly has no climb to show.
        assertNull(WindByHeightWords.climb(here))

        val above = here.copy(floorM = 1800.0, ceilingM = 2000.0, ageS = 20.0, isHere = false)
        assertEquals("Measured just now", WindByHeightWords.detail(above))
        assertEquals("1,800 to 2,000 metres", WindByHeightWords.spokenHeights(above))

        // How fast the glider climbed through a band stands under its
        // wind, and a screen reader says it after the wind.
        val climbed = here.copy(climbMs = 2.4)
        assertEquals("+2.4 m/s", WindByHeightWords.climb(climbed))
        assertEquals(
            "11 kilometres per hour from W, measured 3 minutes ago, climbed +2.4 metres per second",
            WindByHeightWords.spokenWind(climbed),
        )
        // Where the thermal weakened it may have been sinking air: said with its sign.
        assertEquals("\u22120.3 m/s", WindByHeightWords.climb(here.copy(climbMs = -0.3)))
        assertTrue(WindByHeightWords.EXPLANATION.endsWith("how fast you climbed through the band while circling."))
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
        // The sample's thermal lifts at about 2.4 m/s: the bands it was
        // circled through in full say so.
        val climbs = bands.mapNotNull { it.climbMs }
        assertTrue("$bands", climbs.isNotEmpty())
        assertTrue("$bands", climbs.all { it in 1.5..3.5 })
    }
}
