package fly.venti.app.fly

import fly.venti.app.LaunchOptions
import fly.venti.app.TestSupport
import fly.venti.core.FlightSession
import fly.venti.core.Replay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The wind over the cockpit's map: how old it is, in the badge's few
 * letters, and the core's line about a wind that leaves little way.
 */
class WindBadgeTest {
    @Test
    fun aWindsAgeIsShownOnceItIsFiveMinutesOld() {
        assertNull(WindBadgeWords.age(null))
        assertNull(WindBadgeWords.age(40.0))
        assertNull(WindBadgeWords.age(299.0))
        assertEquals("5 min", WindBadgeWords.age(300.0))
        assertEquals("59 min", WindBadgeWords.age(59 * 60 + 59.0))
        assertEquals("1 h", WindBadgeWords.age(3600.0))
        assertEquals("2 h", WindBadgeWords.age(2 * 3600 + 100.0))
    }

    @Test
    fun theSampleFlightsWindAgesOnTheGlideAndIsNoStrongWind() {
        val session = FlightSession(TestSupport.newFolder("wind").path)
        val replay = Replay(TestSupport.sampleFlight)

        // Ten minutes in: on the glide, four minutes after the thermal that
        // gave the wind, which still stands, from the west.
        replay.advance(session, 600.0)
        val view = session.view(replay.now())
        val wind = checkNotNull(view.wind)
        assertEquals(3.0, wind.speedMs, 0.5)
        assertEquals(270.0, wind.fromDeg, 8.0)
        assertEquals(257.0, checkNotNull(view.windAgeS), 20.0)
        // A glider that trims at 38 km/h has way enough into it.
        assertNull(view.strongWind)
        // Not yet old enough to say so.
        assertNull(WindBadgeWords.age(view.windAgeS))
        val fresh = WindBadgeWords.spoken(wind, view.windAgeS)
        assertTrue(fresh, fresh.endsWith(" kilometres per hour from W"))

        // Two minutes on it is, and a screen reader hears it too.
        replay.advance(session, 120.0)
        val later = session.view(replay.now())
        assertEquals("6 min", WindBadgeWords.age(later.windAgeS))
        assertEquals("$fresh, measured 6 min ago", WindBadgeWords.spoken(checkNotNull(later.wind), later.windAgeS))
    }

    @Test
    fun aSlowGliderIsToldWhileTheWindIsFreshThatItLeavesLittleWay() {
        val session = FlightSession(TestSupport.newFolder("wind").path)
        // The glider a UI test flies to see the line: 7 km/h faster than the wind.
        session.setGlider(trimSpeedMs = LaunchOptions.SLOW_GLIDER_TRIM_SPEED / 3.6, glideRatio = 8.0)
        val replay = Replay(TestSupport.sampleFlight)

        // Five minutes in, circling: the wind is measured, and the core says what it leaves.
        replay.advance(session, 300.0)
        assertEquals("Into the wind 7 km/h", session.view(replay.now()).strongWind)

        // On the glide the line stays while the wind is fresh, with its age beside the wind.
        replay.advance(session, 420.0)
        val gliding = session.view(replay.now())
        assertTrue(gliding.strongWind, checkNotNull(gliding.strongWind).startsWith("Into the wind "))
        assertEquals("6 min", WindBadgeWords.age(gliding.windAgeS))

        // A wind older than twenty minutes is not warned from: the shell
        // shows what the core says, and the core says nothing. The sample
        // flight measures its wind to the end, so that is long after it.
        replay.advance(session, 2400.0)
        val late = session.view(replay.now())
        assertTrue(checkNotNull(late.windAgeS) > 1200.0)
        assertNull(late.strongWind)
        assertTrue(checkNotNull(WindBadgeWords.age(late.windAgeS)).endsWith(" min"))
    }
}
