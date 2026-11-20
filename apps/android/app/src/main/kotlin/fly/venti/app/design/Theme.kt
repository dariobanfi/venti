package fly.venti.app.design

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.venti.design.VentiColors
import app.venti.design.VentiDark
import app.venti.design.VentiFlight
import app.venti.design.VentiLight
import app.venti.design.VentiRadius
import app.venti.design.VentiType
import app.venti.design.VentiTypeStyle
import fly.venti.app.R

/**
 * The three themes of the owner's design language, "Valley"
 * (`design/DESIGN.md`): `light` and `dark` follow the phone, and `flight`
 * is the cockpit's from take-off to landing, whatever the phone is set to.
 */
enum class Appearance {
    Light,
    Dark,
    Flight,
    ;

    /** The pack's colour tokens for this theme. */
    val colours: VentiColors
        get() = when (this) {
            Light -> VentiLight
            Dark -> VentiDark
            Flight -> VentiFlight
        }
}

/**
 * Material 3 wearing the design pack's tokens: its colours, Fira Sans at
 * its type styles, and its radii. Material's components keep their structure
 * and behaviour; what they look like comes from `design/tokens`, which the
 * build takes from the repository's `design/` folder. No screen writes a
 * colour, size or radius of its own: it asks [Venti].
 */
@Composable
fun VentiTheme(appearance: Appearance, content: @Composable () -> Unit) {
    val colours = appearance.colours
    CompositionLocalProvider(
        LocalAppearance provides appearance,
        LocalVentiColours provides colours,
        LocalDarkAppearance provides (appearance == Appearance.Dark),
    ) {
        MaterialTheme(
            colorScheme = scheme(colours, dark = appearance == Appearance.Dark),
            typography = if (appearance == Appearance.Flight) FlightTypography else AppTypography,
            shapes = VentiShapes,
            content = content,
        )
    }
}

/** The theme that follows the phone: light or dark. */
@Composable
fun VentiTheme(dark: Boolean, content: @Composable () -> Unit) =
    VentiTheme(if (dark) Appearance.Dark else Appearance.Light, content)

/** The design pack's tokens of the theme in use. */
object Venti {
    val colours: VentiColors
        @Composable @ReadOnlyComposable
        get() = LocalVentiColours.current

    val appearance: Appearance
        @Composable @ReadOnlyComposable
        get() = LocalAppearance.current
}

val LocalVentiColours = staticCompositionLocalOf { VentiLight }
val LocalAppearance = staticCompositionLocalOf { Appearance.Light }

/**
 * Whether the app is drawn in dark appearance. Material's colours follow by
 * themselves; the few data colours with a second value for dark ask here.
 */
val LocalDarkAppearance = staticCompositionLocalOf { false }

/**
 * Text style for numbers: digits of one width, so a value that changes under
 * the finger does not jitter sideways.
 */
val TextStyle.numbers: TextStyle get() = copy(fontFeatureSettings = "tnum")

/** The pack's one family, Fira Sans, from `design/fonts`. */
val FiraSans = FontFamily(
    Font(R.font.fira_sans_regular, FontWeight.Normal),
    Font(R.font.fira_sans_italic, FontWeight.Normal, FontStyle.Italic),
    Font(R.font.fira_sans_medium, FontWeight.Medium),
    Font(R.font.fira_sans_semi_bold, FontWeight.SemiBold),
    Font(R.font.fira_sans_bold, FontWeight.Bold),
    Font(R.font.fira_sans_black_italic, FontWeight.Black, FontStyle.Italic),
)

/**
 * One of the pack's type styles. Sizes are in the units of text, so they
 * follow the phone's text-size setting; the cockpit sets its instruments in
 * fixed sizes of its own.
 */
val VentiTypeStyle.text: TextStyle
    get() = TextStyle(
        fontFamily = FiraSans,
        fontSize = sizePx.sp,
        lineHeight = lineHeightPx.sp,
        fontWeight = FontWeight(weight),
        fontStyle = if (italic) FontStyle.Italic else FontStyle.Normal,
        letterSpacing = letterSpacingEm.em,
        fontFeatureSettings = if (tabular) "tnum" else null,
    )

/** A style at another weight. */
private fun TextStyle.weighted(weight: FontWeight) = copy(fontWeight = weight)

/**
 * Material's roles on the pack's styles. Display is the pack's display and
 * hero; headlines are its title; Material's titles, which bars and rows use,
 * are its heading, and its body and label set semibold; body and label are
 * its body, label and caption.
 */
private val AppTypography = Typography(
    displayLarge = VentiType.display.text,
    displayMedium = VentiType.hero.text,
    displaySmall = VentiType.hero.text,
    headlineLarge = VentiType.title.text,
    headlineMedium = VentiType.title.text,
    headlineSmall = VentiType.heading.text,
    titleLarge = VentiType.heading.text,
    titleMedium = VentiType.body.text.weighted(FontWeight.SemiBold),
    titleSmall = VentiType.label.text.weighted(FontWeight.SemiBold),
    bodyLarge = VentiType.body.text,
    bodyMedium = VentiType.label.text.weighted(FontWeight.Normal),
    bodySmall = VentiType.caption.text,
    labelLarge = VentiType.label.text,
    labelMedium = VentiType.caption.text.weighted(FontWeight.Medium),
    labelSmall = VentiType.caption.text.weighted(FontWeight.Medium),
)

/** In flight nothing is set smaller than the pack's label: no captions. */
private val FlightTypography = AppTypography.copy(
    bodySmall = VentiType.label.text.weighted(FontWeight.Normal),
    labelMedium = VentiType.label.text,
    labelSmall = VentiType.label.text,
)

/** The pack's radii by role: status small, cards and inputs medium, sheets large. */
private val VentiShapes = Shapes(
    extraSmall = RoundedCornerShape(VentiRadius.s),
    small = RoundedCornerShape(VentiRadius.s),
    medium = RoundedCornerShape(VentiRadius.m),
    large = RoundedCornerShape(VentiRadius.l),
    extraLarge = RoundedCornerShape(VentiRadius.l),
)

/** The shape of a button and a chip in the pack: a pill. */
val PillShape = CircleShape

/**
 * Material's colour roles on the pack's tokens. Primary is the pack's
 * primary, indigo on cream; a selected chip, tab or segment is primary as
 * well, so Material's secondary container is too. Material's tertiary is the
 * pack's one accent, coral, for the one action that starts a flight.
 * Surfaces: the page is `bg`, cards and sheets `surface`, inset tracks
 * `surface-raised`; no surface is tinted.
 */
private fun scheme(c: VentiColors, dark: Boolean): ColorScheme {
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = c.primary,
        onPrimary = c.onPrimary,
        primaryContainer = c.surfaceRaised,
        onPrimaryContainer = c.text,
        inversePrimary = c.accent,
        secondary = c.textSecondary,
        onSecondary = c.bg,
        secondaryContainer = c.primary,
        onSecondaryContainer = c.onPrimary,
        tertiary = c.accent,
        onTertiary = c.onAccent,
        tertiaryContainer = c.surfaceRaised,
        onTertiaryContainer = c.accentText,
        background = c.bg,
        onBackground = c.text,
        surface = c.bg,
        onSurface = c.text,
        surfaceVariant = c.surfaceRaised,
        onSurfaceVariant = c.textSecondary,
        surfaceTint = Color.Transparent,
        inverseSurface = c.text,
        inverseOnSurface = c.bg,
        error = c.danger,
        onError = c.onDangerFill,
        errorContainer = c.dangerSoft,
        onErrorContainer = c.danger,
        outline = c.borderStrong,
        outlineVariant = c.border,
        surfaceBright = c.surface,
        surfaceDim = c.bg,
        surfaceContainerLowest = c.surface,
        surfaceContainerLow = c.surface,
        surfaceContainer = c.surface,
        surfaceContainerHigh = c.surfaceRaised,
        surfaceContainerHighest = c.surfaceRaised,
    )
}
