package fly.venti.app.assistant

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The pilot's key in Android's own keystore. It runs on a device, because
 * the keystore exists nowhere else.
 */
@RunWith(AndroidJUnit4::class)
class EncryptedKeyStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    @Test
    fun aKeyIsReadBackUntilItIsRemovedAndIsNotWrittenDownAsItIs() {
        val keys = EncryptedKeyStore.anthropic(context)
        keys.remove()
        assertNull(keys.read())

        assertTrue(keys.save("sk-ant-a-key-for-the-test"))

        // Another reader, as after the app has been closed and opened.
        assertEquals("sk-ant-a-key-for-the-test", EncryptedKeyStore.anthropic(context).read())
        val written = context.getSharedPreferences("venti-assistant", Context.MODE_PRIVATE).all.values
        assertEquals(1, written.size)
        assertFalse(written.joinToString().contains("a-key-for-the-test"))

        // A new key replaces the old one.
        assertTrue(keys.save("another"))
        assertEquals("another", keys.read())

        keys.remove()
        assertNull(keys.read())
        assertNull(EncryptedKeyStore.anthropic(context).read())
    }

    @Test
    fun theKeyForAnotherServiceIsKeptApartFromAnthropics() {
        val anthropic = EncryptedKeyStore.anthropic(context)
        val other = EncryptedKeyStore.otherService(context)
        assertTrue(anthropic.save("a-made-up-anthropic-key"))
        assertTrue(other.save("a-made-up-other-key"))

        // Each is read back on its own, and neither is written down as it is.
        assertEquals("a-made-up-anthropic-key", EncryptedKeyStore.anthropic(context).read())
        assertEquals("a-made-up-other-key", EncryptedKeyStore.otherService(context).read())
        val written = context.getSharedPreferences("venti-assistant-other", Context.MODE_PRIVATE).all.values
        assertEquals(1, written.size)
        assertFalse(written.joinToString().contains("made-up"))

        // Removing one leaves the other alone.
        other.remove()
        assertNull(other.read())
        assertEquals("a-made-up-anthropic-key", anthropic.read())
        anthropic.remove()
        assertNull(anthropic.read())
    }
}
