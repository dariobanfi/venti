package fly.venti.app.account

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fly.venti.app.assistant.EncryptedKeyStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The session's token under a key of Android's own keystore. It runs on a
 * device, because the keystore exists nowhere else.
 */
@RunWith(AndroidJUnit4::class)
class KeystoreSessionStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    @Test
    fun aTokenIsReadBackUntilItIsRemovedAndIsNotWrittenDownAsItIs() {
        val session = KeystoreSessionStore(context)
        session.remove()
        assertNull(session.read())

        assertTrue(session.save("a-made-up-token-of-the-test"))

        // Another reader, as after the app has been closed and opened.
        assertEquals("a-made-up-token-of-the-test", KeystoreSessionStore(context).read())
        // The file that the rules for moving to a new phone leave behind.
        val written = context.getSharedPreferences(KeystoreSessionStore.FILE, Context.MODE_PRIVATE).all.values
        assertEquals(1, written.size)
        assertFalse(written.joinToString().contains("made-up-token"))

        // Signing in again replaces the token.
        assertTrue(session.save("another"))
        assertEquals("another", session.read())

        session.remove()
        assertNull(session.read())
        assertNull(KeystoreSessionStore(context).read())
    }

    @Test
    fun signingOutLeavesTheAssistantsKeyAlone() {
        val keys = EncryptedKeyStore.anthropic(context)
        val session = KeystoreSessionStore(context)
        assertTrue(keys.save("a-made-up-key-of-the-test"))
        assertTrue(session.save("a-made-up-token-of-the-test"))

        // Each secret has a file and a key in the keystore of its own.
        session.remove()
        assertEquals("a-made-up-key-of-the-test", keys.read())

        keys.remove()
        assertNull(keys.read())
    }
}
