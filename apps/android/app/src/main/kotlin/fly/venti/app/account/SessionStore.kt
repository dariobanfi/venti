package fly.venti.app.account

import android.content.Context
import fly.venti.app.SealedStore

/**
 * Where the token of the pilot's session on a Venti server is kept.
 *
 * Whoever holds the token can read and change the pilot's flights on that
 * server, so it is kept where the phone keeps secrets, handed to no file of
 * the core, sent only to that server with the requests that act for the
 * account, and never logged.
 */
interface SessionStore {
    fun read(): String?

    /** Keeps the token, replacing any earlier one. Returns whether it worked. */
    fun save(token: String): Boolean

    fun remove()
}

/**
 * The token on a phone: encrypted under a key in Android's keystore, in a
 * file of the app's own that no backup takes and that does not move to a
 * new phone. [SealedStore] says how; the decision log in
 * `docs/architecture.md` says why this and nothing else.
 */
class KeystoreSessionStore(context: Context) : SessionStore {
    private val sealed = SealedStore(context, file = FILE, alias = "fly.venti.session", entry = "token")

    override fun read(): String? = sealed.read()

    override fun save(token: String): Boolean = sealed.save(token)

    override fun remove() = sealed.remove()

    companion object {
        /**
         * The name of the file the sealed token is in. The rules for what
         * Android may copy to a new phone name it
         * (`res/xml/data_extraction_rules.xml`).
         */
        const val FILE = "venti-session"
    }
}

/** A token that is simply there, or not. Tests use it. */
class MemorySessionStore(private var token: String? = null) : SessionStore {
    override fun read(): String? = token

    override fun save(token: String): Boolean {
        this.token = token
        return true
    }

    override fun remove() {
        token = null
    }
}
