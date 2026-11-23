package fly.venti.app.community

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Flags and names of countries from the codes the server gives. */
class CountriesTest {
    @Test
    fun aCodeIsDrawnAsItsFlagAndNamedInEnglish() {
        assertEquals("🇩🇪", Countries.flag("DE"))
        assertEquals("🇦🇹", Countries.flag("at"))
        assertEquals("Germany", Countries.name("DE"))
        assertEquals("Switzerland", Countries.name("ch"))
        assertEquals("🇫🇷 France", Countries.named("FR"))
    }

    @Test
    fun whatIsNoCodeHasNoFlag() {
        assertNull(Countries.flag(null))
        assertNull(Countries.flag(""))
        assertNull(Countries.flag("DEU"))
        assertNull(Countries.flag("D1"))
        // A code Locale does not name is said as itself.
        assertEquals("QQ", Countries.name("qq"))
    }

    @Test
    fun everyCountryIsOfferedOnceByName() {
        val all = Countries.all
        assertTrue(all.size > 240)
        assertEquals(all.size, all.map { it.first }.toSet().size)
        assertEquals(all.sortedBy { it.second }, all)
        assertTrue(all.contains("AT" to "Austria"))
    }
}
