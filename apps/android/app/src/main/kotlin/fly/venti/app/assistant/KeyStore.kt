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
 * app's own storage. [SealedStore] says how. The key for Anthropic and the
 * key for another service are two secrets, each with a file and a key of
 * its own, so that removing one leaves the other alone.
 */
class EncryptedKeyStore private constructor(private val sealed: SealedStore) : KeyStore {
    override fun read(): String? = sealed.read()

    override fun save(key: String): Boolean = sealed.save(key)

    override fun remove() = sealed.remove()

    companion object {
        /** The key for Anthropic. */
        fun anthropic(context: Context) =
            EncryptedKeyStore(SealedStore(context, file = "venti-assistant", alias = "fly.venti.assistant", entry = "anthropic"))

        /** The key for the other service the pilot set by its address. */
        fun otherService(context: Context) = EncryptedKeyStore(
            SealedStore(context, file = "venti-assistant-other", alias = "fly.venti.assistant.other", entry = "other"),
        )
    }
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
