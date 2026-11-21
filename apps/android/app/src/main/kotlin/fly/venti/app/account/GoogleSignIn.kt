package fly.venti.app.account

import android.content.Context
import androidx.credentials.CredentialManager
import androidx.credentials.CustomCredential
import androidx.credentials.GetCredentialRequest
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.exceptions.NoCredentialException
import com.google.android.libraries.identity.googleid.GetSignInWithGoogleOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import fly.venti.app.Http
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import java.io.IOException
import java.net.URLEncoder

/**
 * Where the app gets a Google ID token from, to hand to Venti: Google, by
 * Android's Credential Manager, or in a UI test the development server's
 * made-up Google. The token is kept no longer than the request it goes in.
 */
fun interface GoogleIdTokens {
    /**
     * Asks for an ID token for a Google account the pilot chooses.
     * `context` is the screen's, over which Google shows its sheet;
     * `server` is Venti's address. Throws [GoogleCancelled] when the pilot
     * closed Google's sheet, and an exception with the reason otherwise.
     */
    suspend fun idToken(context: Context, server: String): String
}

/** The pilot closed Google's sheet: nothing to say. */
class GoogleCancelled : Exception()

/**
 * Google's sign-in through Credential Manager: Google's own sheet with the
 * phone's Google accounts and its "Sign in with Google" button. The token
 * is issued for Venti's web client ID (the server's client ID), which the
 * server checks it against.
 */
class CredentialManagerGoogle(private val serverClientId: String) : GoogleIdTokens {
    override suspend fun idToken(context: Context, server: String): String {
        val request = GetCredentialRequest.Builder()
            .addCredentialOption(GetSignInWithGoogleOption.Builder(serverClientId).build())
            .build()
        val credential = try {
            CredentialManager.create(context).getCredential(context, request).credential
        } catch (_: GetCredentialCancellationException) {
            throw GoogleCancelled()
        } catch (error: NoCredentialException) {
            // No Google account on the phone, or none of Google's services.
            throw IOException("This phone has no Google account to sign in with. Add one in the phone's settings.", error)
        } catch (error: GetCredentialException) {
            throw IOException(error.errorMessage?.toString() ?: error.type, error)
        }
        if (credential !is CustomCredential ||
            credential.type != GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL
        ) {
            throw IOException("Google gave no ID token.")
        }
        return GoogleIdTokenCredential.createFrom(credential.data).idToken
    }
}

/**
 * The development server's made-up Google, for the UI tests: an ID token
 * for `email` from `GET /v1/dev/google-id-token`, which the app sends as it
 * would send Google's. Only a debug build started by a test uses it
 * ([fly.venti.app.LaunchOptions.GOOGLE_TEST_ACCOUNT]); a server that is not
 * in the development mode does not answer it.
 */
class DevelopmentGoogle(private val email: String, private val name: String?) : GoogleIdTokens {
    override suspend fun idToken(context: Context, server: String): String {
        val query = buildString {
            append("email=").append(URLEncoder.encode(email, "UTF-8"))
            name?.let { append("&name=").append(URLEncoder.encode(it, "UTF-8").replace("+", "%20")) }
        }
        val answer = Http.send("GET", "$server/v1/dev/google-id-token?$query", listOf("Accept" to "application/json"), null, 20_000)
        if (answer.status != 200) throw IOException("The development server gave no Google token (${answer.status}).")
        return idTokenIn(answer.body.decodeToString()) ?: throw IOException("The development server gave no Google token.")
    }

    companion object {
        /** The `id_token` of the development server's answer. */
        fun idTokenIn(json: String): String? =
            ((runCatching { Json.parseToJsonElement(json) }.getOrNull() as? JsonObject)?.get("id_token") as? JsonPrimitive)
                ?.content
    }
}
