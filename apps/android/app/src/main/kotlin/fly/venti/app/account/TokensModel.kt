package fly.venti.app.account

import fly.venti.core.TokenCreated
import fly.venti.core.TokenForm
import fly.venti.core.TokenView
import fly.venti.core.TokenWithdrawal
import fly.venti.core.tokenCreateRequest
import fly.venti.core.tokenCreated
import fly.venti.core.tokenForm
import fly.venti.core.tokenNameCheck
import fly.venti.core.tokenWithdrawRequest
import fly.venti.core.tokenWithdrawal
import fly.venti.core.tokenWithdrawn
import fly.venti.core.tokens
import fly.venti.core.tokensRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.time.Instant
import java.time.ZoneOffset

/**
 * "Programs with access": the tokens a pilot gives a logger, a script or
 * another maker's app, so that it acts for them without their password.
 *
 * No rule of it is here: the core says what the form shows, checks a name,
 * puts every request together and reads every answer into words. This
 * class sends what the core gives, with the session's token as the
 * account's other requests go, and keeps nothing: not the list, and least
 * of all a secret, which goes to the screen that shows it once and nowhere
 * else.
 */
class TokensModel(
    private val account: AccountModel,
    /** This device's clock, in seconds since 1970: by it a token is said to have ended. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** What the page and the form show: the core's words, and the things a program can be allowed. */
    val form: TokenForm by lazy { tokenForm() }

    /** What the account answered when asked for its tokens. */
    sealed interface Listed {
        /** The tokens, the newest first; none is the core's `empty` line. */
        data class Tokens(val tokens: List<TokenView>) : Listed

        /** No list: why, in words for the pilot. */
        data class Refused(val words: String) : Listed
    }

    /** What became of the form. */
    sealed interface Given {
        /** The token was made: with its secret, for the one moment it is shown. */
        class Made(val created: TokenCreated) : Given

        /** It was not: why, in the core's words. */
        data class Refused(val words: String) : Given
    }

    /** The account's tokens. */
    suspend fun list(): Listed = try {
        val answer = account.answer(withContext(background) { tokensRequest() })
        Listed.Tokens(withContext(background) { tokens(answer.status16, answer.body, now()) })
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        Listed.Refused(account.words(error))
    }

    /** What is wrong with a name, in the core's words; `null` when it can be sent. */
    fun nameFault(name: String): String? = try {
        tokenNameCheck(name)
        null
    } catch (error: Exception) {
        account.words(error)
    }

    /**
     * Makes a token. `scopes` are the names of the form's scopes the pilot
     * ticked, `endsUnixS` the moment from which it is no longer good, if
     * the pilot gave an end. What the core refuses is not sent. The
     * password goes into the one request and is kept nowhere.
     */
    suspend fun give(name: String, scopes: List<String>, endsUnixS: Long?, password: String): Given = try {
        val request = withContext(background) { tokenCreateRequest(name, scopes, endsUnixS, password) }
        val answer = account.answer(request)
        Given.Made(withContext(background) { tokenCreated(answer.status16, answer.body, now()) })
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        Given.Refused(account.words(error))
    }

    /** The words that ask before a token is withdrawn. */
    fun withdrawal(token: TokenView): TokenWithdrawal = tokenWithdrawal(token.name)

    /** Withdraws a token. `null` when it is withdrawn; otherwise why not, in words for the pilot. */
    suspend fun withdraw(token: TokenView): String? = try {
        val answer = account.answer(withContext(background) { tokenWithdrawRequest(token.id) })
        withContext(background) { tokenWithdrawn(answer.status16, answer.body) }
        null
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        account.words(error)
    }

    private val fly.venti.app.HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
}

/** What the page of the programs with access says, apart from what the core has worded. */
object TokenWords {
    const val ENDED = "Ended"
    const val NO_END = "No end"
    const val COPIED = "Copied"

    /**
     * Under a token's name: what it may do, and when it was made, last used
     * and ends, each after the word the core names for it.
     */
    fun detail(token: TokenView): String = listOf(
        token.scopesLine,
        "Made ${token.made}",
        "Last used ${token.lastUsed}",
        "${if (token.hasEnded) ENDED else "Ends"} ${token.ends}",
    ).joinToString("\n")

    /**
     * The moment a token ends when the pilot chose a day: the last second
     * of that day, UTC, the day the core then names in the list. `utcMillis`
     * is a date picker's moment, midnight UTC of the day.
     */
    fun endOfDay(utcMillis: Long): Long = utcMillis / 1000 + SECONDS_OF_A_DAY - 1

    /** The chosen day as the core writes a token's days: "21 Dec 2026". */
    fun day(utcMillis: Long): String = Instant.ofEpochMilli(utcMillis).atZone(ZoneOffset.UTC).let { day ->
        "${day.dayOfMonth} ${MONTHS[day.monthValue - 1]} ${day.year}"
    }

    /** Whether a date picker's day can be chosen as an end: a day that has not begun, UTC. */
    fun canEndOn(utcMillis: Long, nowUnixS: Long): Boolean = utcMillis / 1000 > nowUnixS

    private const val SECONDS_OF_A_DAY = 86_400L
    private val MONTHS = listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")
}
