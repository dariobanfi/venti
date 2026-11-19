package fly.venti.app.assistant

import android.content.Context
import fly.venti.app.SealedStore

/**
 * Where the pilot's key for their model provider is kept.
 *
 * It is read when a question is asked and handed to the core for that one
 * request. It is never sent anywhere but to the provider.
 */
interface KeyStore {
    fun read(): String?

    /** Stores the key, replacing any earlier one. Returns whether it worked. */
    fun save(key: String): Boolean

    fun remove()
}

/**
 * The key on a phone: encrypted under a key in Android's keystore, in the
 * app's own storage. [SealedStore] says how.
 */
class EncryptedKeyStore(context: Context) : KeyStore {
    private val sealed = SealedStore(context, file = "venti-assistant", alias = "fly.venti.assistant", entry = "anthropic")

    override fun read(): String? = sealed.read()

    override fun save(key: String): Boolean = sealed.save(key)

    override fun remove() = sealed.remove()
}

/** A key that is simply there, or not. Tests use it. */
class FixedKey(private var key: String?) : KeyStore {
    override fun read(): String? = key

    override fun save(key: String): Boolean {
        this.key = key
        return true
    }

    override fun remove() {
        key = null
    }
}
