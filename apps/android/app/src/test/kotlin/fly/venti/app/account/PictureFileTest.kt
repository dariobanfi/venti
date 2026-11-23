package fly.venti.app.account

import org.junit.Assert.assertEquals
import org.junit.Test

/** How a chosen photo is made smaller before it is sent: the arithmetic, without Android's decoder. */
class PictureFileTest {
    @Test
    fun aPhotoIsDecodedAtThePowerOfTwoThatKeepsItLargeEnough() {
        assertEquals(1, PictureFile.sampleSize(800, 600, 1024))
        assertEquals(1, PictureFile.sampleSize(2047, 1000, 1024))
        assertEquals(2, PictureFile.sampleSize(2048, 1536, 1024))
        assertEquals(2, PictureFile.sampleSize(4000, 3000, 1024))
        assertEquals(8, PictureFile.sampleSize(3000, 8192, 1024))
        assertEquals(4, PictureFile.sampleSize(3000, 8191, 1024))
    }

    @Test
    fun theLongerSideIsScaledToTheMostAndNeverUp() {
        assertEquals(800 to 600, PictureFile.scaled(800, 600, 1024))
        assertEquals(1024 to 768, PictureFile.scaled(2000, 1500, 1024))
        assertEquals(576 to 1024, PictureFile.scaled(1512, 2688, 1024))
        assertEquals(1024 to 1, PictureFile.scaled(10_000, 3, 1024))
    }
}
