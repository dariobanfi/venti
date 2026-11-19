package fly.venti.app.design

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle

/**
 * Material 3 with Venti's one accent.
 *
 * The scheme is fixed, not taken from the pilot's wallpaper: colour in this
 * app carries meaning or is not used, and a wallpaper's orange or blue would
 * sit beside the climb colours as if it meant something. The neutral
 * surfaces carry a trace of the accent, as Material's own do.
 */
@Composable
fun VentiTheme(dark: Boolean, content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (dark) DarkColours else LightColours, content = content)
}

/**
 * Text style for numbers: digits of one width, so a value that changes under
 * the finger does not jitter sideways. Android has no rounded system face,
 * so numbers keep the system's own.
 */
val TextStyle.numbers: TextStyle get() = copy(fontFeatureSettings = "tnum")

private val LightColours: ColorScheme = lightColorScheme(
    primary = Palette.accentLight,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE4DFFF),
    onPrimaryContainer = Color(0xFF1B0E63),
    inversePrimary = Color(0xFFC6BFFF),
    secondary = Color(0xFF5D5B72),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE3DFF9),
    onSecondaryContainer = Color(0xFF1A182C),
    // No third hue: a stray pink or brown would look like data.
    tertiary = Color(0xFF5D5B72),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFE3DFF9),
    onTertiaryContainer = Color(0xFF1A182C),
    background = Color(0xFFFDFBFF),
    onBackground = Color(0xFF1B1B1F),
    surface = Color(0xFFFDFBFF),
    onSurface = Color(0xFF1B1B1F),
    surfaceVariant = Color(0xFFE5E1EC),
    onSurfaceVariant = Color(0xFF47464F),
    surfaceTint = Palette.accentLight,
    inverseSurface = Color(0xFF303034),
    inverseOnSurface = Color(0xFFF3EFF4),
    outline = Color(0xFF787680),
    outlineVariant = Color(0xFFC9C5D0),
    surfaceBright = Color(0xFFFDFBFF),
    surfaceDim = Color(0xFFDCD9E0),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFF7F5FA),
    surfaceContainer = Color(0xFFF1EFF4),
    surfaceContainerHigh = Color(0xFFEBE9EF),
    surfaceContainerHighest = Color(0xFFE5E3E9),
)

private val DarkColours: ColorScheme = darkColorScheme(
    primary = Palette.accentDark,
    // White on the lighter accent is hard to read; near-black is not.
    onPrimary = Color(0xFF110B45),
    primaryContainer = Color(0xFF453AA6),
    onPrimaryContainer = Color(0xFFE4DFFF),
    inversePrimary = Palette.accentLight,
    secondary = Color(0xFFC7C3DC),
    onSecondary = Color(0xFF2F2D42),
    secondaryContainer = Color(0xFF46445A),
    onSecondaryContainer = Color(0xFFE3DFF9),
    tertiary = Color(0xFFC7C3DC),
    onTertiary = Color(0xFF2F2D42),
    tertiaryContainer = Color(0xFF46445A),
    onTertiaryContainer = Color(0xFFE3DFF9),
    background = Color(0xFF131316),
    onBackground = Color(0xFFE5E1E6),
    surface = Color(0xFF131316),
    onSurface = Color(0xFFE5E1E6),
    surfaceVariant = Color(0xFF47464F),
    onSurfaceVariant = Color(0xFFC9C5D0),
    surfaceTint = Palette.accentDark,
    inverseSurface = Color(0xFFE5E1E6),
    inverseOnSurface = Color(0xFF303034),
    outline = Color(0xFF928F9A),
    outlineVariant = Color(0xFF47464F),
    surfaceBright = Color(0xFF39393C),
    surfaceDim = Color(0xFF131316),
    surfaceContainerLowest = Color(0xFF0E0E11),
    surfaceContainerLow = Color(0xFF1B1B1F),
    surfaceContainer = Color(0xFF1F1F23),
    surfaceContainerHigh = Color(0xFF2A292D),
    surfaceContainerHighest = Color(0xFF353438),
)
