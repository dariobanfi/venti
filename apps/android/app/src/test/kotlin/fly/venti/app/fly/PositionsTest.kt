package fly.venti.app.fly

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The cockpit flies by fixes of now only: a receiver that wakes hands on old and rough positions first. */
class PositionsTest {
    @Test
    fun onlyAnAccurateFixOfNowIsUsed() {
        assertTrue(Positions.isUsable(accuracyM = 5.0, ageS = 0.5))
        assertTrue(Positions.isUsable(accuracyM = 100.0, ageS = 10.0))
        // Accuracy unknown, or worse than a hundred metres: a guess.
        assertFalse(Positions.isUsable(accuracyM = null, ageS = 0.5))
        assertFalse(Positions.isUsable(accuracyM = 250.0, ageS = 0.5))
        assertFalse(Positions.isUsable(accuracyM = -1.0, ageS = 0.5))
        // Where the receiver was a minute ago is not where it is.
        assertFalse(Positions.isUsable(accuracyM = 5.0, ageS = 60.0))
    }
}
