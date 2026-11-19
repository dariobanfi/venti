package fly.venti.app.assistant

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.core.content.edit
import java.security.GeneralSecurityException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

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
 * The key on a phone: encrypted in the app's own storage.
 *
 * Android has no store for secrets as such. It has a keystore for
 * encryption keys, which never leave it and on most phones live in separate
 * hardware. So the pilot's key is encrypted with such a key and only the
 * result is written down, in a file no other app can read and no backup
 * takes.
 */
class EncryptedKeyStore(context: Context) : KeyStore {
    private val store = context.getSharedPreferences("venti-assistant", Context.MODE_PRIVATE)

    override fun read(): String? {
        val sealed = store.getString(SEALED_KEY, null) ?: return null
        return try {
            val bytes = Base64.decode(sealed, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secret(), GCMParameterSpec(TAG_BITS, bytes, 0, NONCE_BYTES))
            cipher.doFinal(bytes, NONCE_BYTES, bytes.size - NONCE_BYTES).decodeToString()
        } catch (_: GeneralSecurityException) {
            // The keystore's key is gone, or the stored text is damaged:
            // the pilot enters the key again.
            null
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    override fun save(key: String): Boolean =
        try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, secret())
            // The nonce the keystore chose goes in front of what it encrypted.
            val sealed = cipher.iv + cipher.doFinal(key.toByteArray())
            store.edit { putString(SEALED_KEY, Base64.encodeToString(sealed, Base64.NO_WRAP)) }
            true
        } catch (_: GeneralSecurityException) {
            false
        }

    override fun remove() {
        store.edit { remove(SEALED_KEY) }
        try {
            keystore().deleteEntry(ALIAS)
        } catch (_: GeneralSecurityException) {
            // Nothing can be read with it any more either way.
        }
    }

    private fun keystore(): java.security.KeyStore =
        java.security.KeyStore.getInstance(KEYSTORE).apply { load(null) }

    /** The encryption key in Android's keystore, made on first use. */
    private fun secret(): SecretKey {
        (keystore().getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "fly.venti.assistant"
        const val SEALED_KEY = "anthropic"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val TAG_BITS = 128
        const val NONCE_BYTES = 12
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
