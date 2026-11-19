package fly.venti.app

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.core.content.edit
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * One secret on a phone, encrypted in the app's own storage: the pilot's
 * key for the assistant, the token of the session on a Venti server.
 *
 * Android has no store for secrets as such. It has a keystore for
 * encryption keys, which never leave it and on most phones live in separate
 * hardware. So the secret is encrypted with such a key (AES-256 in GCM) and
 * only the result is written down, in a file of the app's own that no other
 * app can read and no backup takes. Nothing here is logged.
 *
 * Each secret has a file and a key of its own, so that removing one leaves
 * the other alone.
 */
class SealedStore(
    context: Context,
    /** The name of the file of the app's that holds the sealed secret. */
    file: String,
    /** The name of the encryption key in Android's keystore. */
    private val alias: String,
    /** What the secret is called in its file. */
    private val entry: String,
) {
    private val store = context.getSharedPreferences(file, Context.MODE_PRIVATE)

    /** The secret, or `null` if none is kept or what is kept can no longer be read. */
    fun read(): String? {
        val sealed = store.getString(entry, null) ?: return null
        return try {
            val bytes = Base64.decode(sealed, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secret(), GCMParameterSpec(TAG_BITS, bytes, 0, NONCE_BYTES))
            cipher.doFinal(bytes, NONCE_BYTES, bytes.size - NONCE_BYTES).decodeToString()
        } catch (_: GeneralSecurityException) {
            // The keystore's key is gone, as on a phone the app's files
            // were moved to, or the stored text is damaged: the pilot
            // enters the secret again.
            null
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    /** Keeps the secret, replacing any earlier one. Returns whether it worked. */
    fun save(secret: String): Boolean =
        try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, secret())
            // The nonce the keystore chose goes in front of what it encrypted.
            val sealed = cipher.iv + cipher.doFinal(secret.toByteArray())
            store.edit { putString(entry, Base64.encodeToString(sealed, Base64.NO_WRAP)) }
            true
        } catch (_: GeneralSecurityException) {
            false
        }

    /** Forgets the secret, and the key it was encrypted with. */
    fun remove() {
        store.edit { remove(entry) }
        try {
            keystore().deleteEntry(alias)
        } catch (_: GeneralSecurityException) {
            // Nothing can be read with it any more either way.
        }
    }

    private fun keystore(): KeyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }

    /** The encryption key in Android's keystore, made on first use. */
    private fun secret(): SecretKey {
        (keystore().getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val TAG_BITS = 128
        const val NONCE_BYTES = 12
    }
}
