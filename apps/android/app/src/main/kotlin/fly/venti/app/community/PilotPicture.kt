package fly.venti.app.community

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSpace
import fly.venti.app.Http
import fly.venti.app.design.Venti
import fly.venti.core.PictureWords
import fly.venti.core.pictureWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The pilots' pictures: fetched from Venti by their paths and kept while
 * the app runs. A path names one picture for good (the core says so), so
 * what was fetched for a path is never fetched again in a run.
 *
 * A pilot's page shows the picture another list named for the handle in
 * this run until the page's own totals have come.
 */
object PilotPictures {
    /** Venti's address, without a slash at its end: set by the session, as the lists' is. */
    @Volatile
    var server: () -> String? = { null }

    /** The words of a picture and of the pilots near a place: the core's. */
    val words: PictureWords by lazy { pictureWords() }

    /** The pictures fetched in this run, by path: 256 pixels square each, so a hundred are some 25 MB at most. */
    private val fetched = LruCache<String, ImageBitmap>(KEPT)

    /** The path of each pilot's picture as the core last named it in this run: `null` for a pilot without one. */
    private val named = mutableStateMapOf<String, String?>()

    /** What the core said of a pilot's picture: kept for their page, until its totals come. */
    fun know(handle: String, path: String?) {
        if (handle.isEmpty()) return
        if (!named.containsKey(handle) || named[handle] != path) named[handle] = path
    }

    /** The path of a pilot's picture as the core last named it, if it has in this run. */
    fun of(handle: String): String? = named[handle]

    /** A picture fetched before, if there is one. */
    fun kept(path: String): ImageBitmap? = fetched.get(path)

    /**
     * Fetches a picture by its path, or hands back the one fetched before;
     * `null` where there is no server or the picture cannot be had. The
     * face shows the initials then.
     */
    suspend fun fetch(path: String): ImageBitmap? {
        fetched.get(path)?.let { return it }
        val server = server() ?: return null
        return try {
            val answer = Http.get(server + path, accept = "image/webp,image/*")
            if (answer.status != 200) return null
            val picture = withContext(Dispatchers.Default) {
                BitmapFactory.decodeByteArray(answer.body, 0, answer.body.size)?.asImageBitmap()
            } ?: return null
            fetched.put(path, picture)
            picture
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
    }

    /**
     * The initials of a name, for a face without a picture: the first
     * letter of its first two words, as the core makes them for the pilots
     * near a place ("Lena Probe" is "LP").
     */
    fun initials(name: String): String =
        name.split(' ', '\t', '\n')
            .filter { it.isNotBlank() }
            .mapNotNull { word -> word.firstOrNull { it.isLetterOrDigit() } }
            .take(2)
            .joinToString("") { it.uppercase() }

    private const val KEPT = 100
}

/** How large a face is drawn. */
enum class FaceSize(val side: Dp) {
    /** Beside a name in a row. */
    Small(VentiSpace.s6),

    /** In a card or a list of pilots. */
    Medium(VentiSpace.s7),

    /** At the head of a pilot's page and of the account. */
    Large(VentiSpace.s8),
}

/**
 * A pilot's face: their picture in a circle, or, without one, their
 * initials in a circle. It is decoration: the name beside it says who it
 * is, so a screen reader passes over it.
 */
@Composable
fun PilotFace(
    name: String,
    picturePath: String?,
    modifier: Modifier = Modifier,
    size: FaceSize = FaceSize.Small,
    /** The initials where the core made them; else the shell makes them from the name. */
    initials: String? = null,
) {
    val colours = Venti.colours
    var picture by remember(picturePath) { mutableStateOf(picturePath?.let(PilotPictures::kept)) }
    LaunchedEffect(picturePath) {
        if (picturePath != null && picture == null) picture = PilotPictures.fetch(picturePath)
    }
    Box(
        modifier
            .size(size.side)
            .clip(CircleShape)
            .background(colours.surfaceRaised)
            .border(1.dp, colours.border, CircleShape)
            .testTag(if (picture != null) "pilot-face-picture" else "pilot-face-initials")
            .clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        val shown = picture
        if (shown != null) {
            Image(shown, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.size(size.side))
        } else {
            val letters = initials?.takeIf { it.isNotEmpty() } ?: PilotPictures.initials(name)
            Text(
                letters,
                style = when (size) {
                    FaceSize.Small -> MaterialTheme.typography.labelMedium
                    FaceSize.Medium -> MaterialTheme.typography.titleSmall
                    FaceSize.Large -> MaterialTheme.typography.headlineMedium
                },
                fontWeight = FontWeight.SemiBold,
                color = colours.textSecondary,
                maxLines = 1,
            )
        }
    }
}
