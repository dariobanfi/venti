package fly.venti.app.community

import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import java.util.IllformedLocaleException
import java.util.Locale

/**
 * Countries as the server names them, by their ISO 3166-1 alpha-2 codes: a
 * flag drawn from the code and a name in English, as on the web.
 *
 * The flag is the pair of regional indicator letters of the code, which
 * the phone's own emoji font draws as a flag; no picture of a flag is in
 * the app. The name is `java.util.Locale`'s, in English as the rest of the
 * app, and the code itself where it has none.
 */
object Countries {
    /** The flag of a country from its two letters; `null` for no code or one that is not two letters. */
    fun flag(code: String?): String? {
        val letters = code?.trim()?.uppercase(Locale.ROOT) ?: return null
        if (letters.length != 2 || !letters.all { it in 'A'..'Z' }) return null
        return letters.map { letter -> String(Character.toChars(REGIONAL_A + (letter - 'A'))) }.joinToString("")
    }

    /** A country's name in English: "Germany" for `DE`; the code where `Locale` knows none. */
    fun name(code: String): String {
        val letters = code.trim().uppercase(Locale.ROOT)
        val name = try {
            Locale.Builder().setRegion(letters).build().getDisplayCountry(Locale.ENGLISH)
        } catch (_: IllformedLocaleException) {
            ""
        }
        return name.ifEmpty { letters }
    }

    /** A country with its flag before its name, "🇩🇪 Germany", for a row that names it. */
    fun named(code: String): String = listOfNotNull(flag(code), name(code)).joinToString(" ")

    /** Every country `Locale` knows, by its name, for the pilot to choose from. */
    val all: List<Pair<String, String>> by lazy {
        Locale.getISOCountries().map { it to name(it) }.sortedBy { it.second }
    }

    /** The first regional indicator symbol, the letter A. */
    private const val REGIONAL_A = 0x1F1E6
}

/** A country's flag, named for a screen reader; nothing without a code. */
@Composable
fun Flag(code: String?, modifier: Modifier = Modifier, style: TextStyle = LocalTextStyle.current) {
    val drawn = Countries.flag(code) ?: return
    val name = Countries.name(checkNotNull(code))
    Text(drawn, style = style, maxLines = 1, modifier = modifier.semantics { contentDescription = name })
}
