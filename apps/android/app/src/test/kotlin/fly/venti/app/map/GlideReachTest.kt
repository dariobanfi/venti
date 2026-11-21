package fly.venti.app.map

import fly.venti.app.TestSupport
import fly.venti.app.terrain.TerrainModel
import fly.venti.core.Coordinate
import fly.venti.core.ReachEnd
import fly.venti.core.ReachRay
import java.util.Locale
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * What a glide reaches from the Wallberg, against the real core and the
 * ground of the region that comes with the app; and the steps of the
 * card's settings and the edge as the map draws it.
 */
class GlideReachTest {
    private val locale = Locale.getDefault()

    @Before
    fun useEnglish() {
        Locale.setDefault(Locale.UK)
    }

    @After
    fun restore() {
        Locale.setDefault(locale)
    }

    private val wallberg = Coordinate(47.6642, 11.7967)

    private fun ground(): TerrainModel {
        val archive = TestSupport.assets.resolve("MapDemo").resolve(MapStyle.BUNDLED_TERRAIN)
        return TerrainModel(source = { TerrainModel.Source.Archive(archive) }, folder = null)
    }

    private fun model(terrain: TerrainModel?) =
        GlideReachModel(ground = terrain?.let { ground -> { area -> ground.covering(area) } })

    @Test
    fun fromTheWallbergsTopTheCoreSaysWhereAGlideReaches() = runBlocking {
        val reach = model(ground())
        reach.show(wallberg, glideRatio = 8.0)
        reach.ask()
        val answer = checkNotNull(reach.answer)
        assertTrue(answer.words, answer.words.startsWith("From "))
        assertTrue(answer.outline.size > 3)
        assertEquals(answer.outline.first(), answer.outline.last())
        assertNotNull(answer.groundM)
        assertEquals(answer.groundM!!, answer.startM!!, 1e-6)
        assertTrue(answer.note.isNotEmpty())
        assertFalse(reach.isAsking)

        // Higher, with the settings' steps: asked again without a server.
        reach.startHeightM = GlideReachWords.stepStart(null, answer.groundM, 2)
        reach.ask()
        val higher = reach.answer!!
        assertEquals(reach.startHeightM!!, higher.startM!!, 1e-6)
        assertTrue(higher.farthestM >= answer.farthestM)

        // A wind is asked with its direction and speed.
        reach.wind = GlideReachWords.FIRST_WIND
        val setting = reach.setting()!!
        assertEquals(270.0, setting.windFromDeg!!, 1e-9)
        assertEquals(15.0, setting.windKmh!!, 1e-9)
        assertNull(setting.marginM)

        reach.clear()
        assertNull(reach.place)
        assertNull(reach.answer)
        assertNull(reach.setting())
        reach.close()
    }

    @Test
    fun withoutTheGroundTheCoreSaysItIsNotLoaded() = runBlocking {
        val reach = model(null)
        reach.show(wallberg, glideRatio = 8.0)
        reach.ask()
        val answer = reach.answer!!
        assertTrue(answer.outline.isEmpty())
        assertTrue(answer.words, answer.words.contains("not loaded"))
        reach.close()
    }

    @Test
    fun theSettingsMoveByTheirSteps() {
        // From the ground at 1717 m up to whole steps of 50 m, and back to the ground.
        assertEquals(1750.0, GlideReachWords.stepStart(null, 1717.0, 1)!!, 1e-9)
        assertEquals(1800.0, GlideReachWords.stepStart(1750.0, 1717.0, 1)!!, 1e-9)
        assertNull(GlideReachWords.stepStart(1750.0, 1717.0, -1))
        assertNull(GlideReachWords.stepStart(null, 1717.0, -1))
        assertNull(GlideReachWords.stepStart(null, null, 1))
        assertEquals(9000.0, GlideReachWords.stepStart(9000.0, 1717.0, 1)!!, 1e-9)
        assertEquals("On the ground", GlideReachWords.start(null))
        assertEquals("1750 m", GlideReachWords.start(1750.0))
        assertEquals(8.5, GlideReachWords.stepGlide(8.0, 1), 1e-9)

        val wind = GlideReachWords.FIRST_WIND
        assertEquals("W", GlideReachWords.windFrom(wind))
        assertEquals("NW", GlideReachWords.windFrom(GlideReachWords.stepWindFrom(wind, 1)))
        assertEquals(0.0, GlideReachWords.stepWindFrom(GlideReachWords.stepWindFrom(wind, 1), 1).fromDeg, 1e-9)
        assertEquals(225.0, GlideReachWords.stepWindFrom(wind, -1).fromDeg, 1e-9)
        assertEquals("20 km/h", GlideReachWords.windSpeed(GlideReachWords.stepWindSpeed(wind, 1)))
        assertEquals(5, GlideReachWords.stepWindSpeed(wind.copy(kmh = 5), -1).kmh)

        // The ground is brought for a box round the place, wider east and west.
        val box = GlideReachWords.around(wallberg, 0.15)
        assertEquals(47.5142, box.south, 1e-9)
        assertTrue(box.east - box.west > box.north - box.south)
    }

    @Test
    fun theEdgeIsDashedBetweenDirectionsThatRanOffTheGround() {
        fun ray(bearing: Double, end: ReachEnd) = ReachRay(
            bearingDeg = bearing,
            distanceM = 1000.0,
            endAt = Coordinate(lat = bearing, lon = 0.0),
            arrivalM = 1000.0,
            overGroundM = null,
            end = end,
        )
        val rays = listOf(
            ray(0.0, ReachEnd.GROUND),
            ray(1.0, ReachEnd.NOT_LOADED),
            ray(2.0, ReachEnd.NOT_LOADED),
            ray(3.0, ReachEnd.NOT_LOADED),
            ray(4.0, ReachEnd.GROUND),
        )
        val edges = GlideReachWords.edges(rays)
        assertEquals(listOf(listOf(1.0, 2.0, 3.0)), edges.dashed.map { line -> line.map { it.lat } })
        assertEquals(listOf(listOf(0.0, 1.0), listOf(3.0, 4.0, 0.0)), edges.solid.map { line -> line.map { it.lat } })
        // All on the ground: one solid ring.
        val ground = GlideReachWords.edges(rays.map { it.copy(end = ReachEnd.GROUND) })
        assertTrue(ground.dashed.isEmpty())
        assertEquals(listOf(0.0, 1.0, 2.0, 3.0, 4.0, 0.0), ground.solid.single().map { it.lat })
        assertTrue(GlideReachWords.edges(emptyList()).solid.isEmpty())
    }
}
