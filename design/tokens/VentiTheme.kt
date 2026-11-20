// Venti tokens for Jetpack Compose.
package app.venti.design

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

object VentiBrand {
    val Ink = Color(0xFF221640)
    val Indigo = Color(0xFF302157)
    val Purple = Color(0xFF5A4284)
    val Violet = Color(0xFF6B518F)
    val Mauve = Color(0xFFA276A4)
    val Lilac = Color(0xFFB0A5C7)
    val Sky = Color(0xFF40ADB9)
    val Mint = Color(0xFF99D5CA)
    val Sage = Color(0xFFDAE4C5)
    val Cream = Color(0xFFFDEDC5)
    val Sand = Color(0xFFFBD5B1)
    val Peach = Color(0xFFF3A682)
    val Gold = Color(0xFFFDD283)
    val Orange = Color(0xFFF8A65A)
    val Coral = Color(0xFFE56954)
    val Brick = Color(0xFFD65F56)
    val Rose = Color(0xFFB9466F)
}

data class VentiColors(
    val bg: Color,
    val surface: Color,
    val surfaceRaised: Color,
    val border: Color,
    val borderStrong: Color,
    val text: Color,
    val textSecondary: Color,
    val textMuted: Color,
    val primary: Color,
    val primaryPressed: Color,
    val onPrimary: Color,
    val accent: Color,
    val onAccent: Color,
    val accentText: Color,
    val link: Color,
    val focus: Color,
    val good: Color,
    val goodSoft: Color,
    val caution: Color,
    val cautionSoft: Color,
    val danger: Color,
    val dangerSoft: Color,
    val dangerFill: Color,
    val onDangerFill: Color,
    val series: List<Color>,
    val vario: List<Color>,
    val wind: List<Color>,
    val thermal: List<Color>,
)

val VentiLight = VentiColors(
    bg = Color(0xFFFFF8E8),
    surface = Color(0xFFFFFDF7),
    surfaceRaised = Color(0xFFF8ECD3),
    border = Color(0xFFE6D8C0),
    borderStrong = Color(0xFF8A7CA6),
    text = Color(0xFF221640),
    textSecondary = Color(0xFF54457A),
    textMuted = Color(0xFF6C5F8C),
    primary = Color(0xFF302157),
    primaryPressed = Color(0xFF221640),
    onPrimary = Color(0xFFFDEDC5),
    accent = Color(0xFFE56954),
    onAccent = Color(0xFF1A0F33),
    accentText = Color(0xFFB23A2A),
    link = Color(0xFF0A6874),
    focus = Color(0xFF0A7F8E),
    good = Color(0xFF17714B),
    goodSoft = Color(0xFFDCEFDD),
    caution = Color(0xFF8A5600),
    cautionSoft = Color(0xFFFDE9B8),
    danger = Color(0xFFB5123A),
    dangerSoft = Color(0xFFFBDDE0),
    dangerFill = Color(0xFFB5123A),
    onDangerFill = Color(0xFFFFFFFF),
    series = listOf(Color(0xFF6A4CB8), Color(0xFFE2593F), Color(0xFF0C93A3), Color(0xFFB9800A), Color(0xFFA63A66)),
    vario = listOf(Color(0xFF0A5F6C), Color(0xFF1F93A1), Color(0xFF86C9CC), Color(0xFFCBC6C2), Color(0xFFF3A682), Color(0xFFE56954), Color(0xFFB02A45)),
    wind = listOf(Color(0xFFE9E3F2), Color(0xFFCFC3E3), Color(0xFFB0A0D0), Color(0xFF8F78BA), Color(0xFF6F56A3), Color(0xFF4F3A82), Color(0xFF302157)),
    thermal = listOf(Color(0xFFFDE7CF), Color(0xFFFBCDA6), Color(0xFFF6A97C), Color(0xFFEC8260), Color(0xFFD95A4A), Color(0xFFA93640)),
)

val VentiDark = VentiColors(
    bg = Color(0xFF1C1236),
    surface = Color(0xFF2A1C4F),
    surfaceRaised = Color(0xFF3A2A68),
    border = Color(0xFF4A3880),
    borderStrong = Color(0xFF8F80B8),
    text = Color(0xFFFFF4DA),
    textSecondary = Color(0xFFCFC4E2),
    textMuted = Color(0xFFAFA2CC),
    primary = Color(0xFFFDEDC5),
    primaryPressed = Color(0xFFFBD5B1),
    onPrimary = Color(0xFF221640),
    accent = Color(0xFFF0806B),
    onAccent = Color(0xFF1A0F33),
    accentText = Color(0xFFFF9C86),
    link = Color(0xFF7CD6D8),
    focus = Color(0xFF7CD6D8),
    good = Color(0xFF6BD9A4),
    goodSoft = Color(0xFF1F4A48),
    caution = Color(0xFFFDD283),
    cautionSoft = Color(0xFF54402F),
    danger = Color(0xFFFF93A4),
    dangerSoft = Color(0xFF5A1F45),
    dangerFill = Color(0xFFC2183F),
    onDangerFill = Color(0xFFFFFFFF),
    series = listOf(Color(0xFF9B7FE0), Color(0xFFE8684E), Color(0xFF19A2AF), Color(0xFFBE8A1E), Color(0xFFC94E86)),
    vario = listOf(Color(0xFF9BE8E4), Color(0xFF40ADB9), Color(0xFF35788F), Color(0xFF5F5A72), Color(0xFFB0605C), Color(0xFFF0806B), Color(0xFFFDC98A)),
    wind = listOf(Color(0xFF43336F), Color(0xFF58458C), Color(0xFF6F59A6), Color(0xFF8C77C0), Color(0xFFAA98D6), Color(0xFFC9BCE8), Color(0xFFEAE3F8)),
    thermal = listOf(Color(0xFF54304F), Color(0xFF7E3E54), Color(0xFFAC5157), Color(0xFFD96A58), Color(0xFFF59270), Color(0xFFFDC49C)),
)

val VentiFlight = VentiColors(
    bg = Color(0xFFFFFFFF),
    surface = Color(0xFFFFFFFF),
    surfaceRaised = Color(0xFFEFEAF5),
    border = Color(0xFF120A24),
    borderStrong = Color(0xFF120A24),
    text = Color(0xFF120A24),
    textSecondary = Color(0xFF221640),
    textMuted = Color(0xFF3B2C63),
    primary = Color(0xFF120A24),
    primaryPressed = Color(0xFF302157),
    onPrimary = Color(0xFFFFFFFF),
    accent = Color(0xFFE56954),
    onAccent = Color(0xFF120A24),
    accentText = Color(0xFF8A2215),
    link = Color(0xFF06525C),
    focus = Color(0xFF06525C),
    good = Color(0xFF084F30),
    goodSoft = Color(0xFFD3EFDD),
    caution = Color(0xFF603B00),
    cautionSoft = Color(0xFFFFE39A),
    danger = Color(0xFF880826),
    dangerSoft = Color(0xFFFFDCE1),
    dangerFill = Color(0xFFA00A2E),
    onDangerFill = Color(0xFFFFFFFF),
    series = listOf(Color(0xFF6A4CB8), Color(0xFFE2593F), Color(0xFF0C93A3), Color(0xFFB9800A), Color(0xFFA63A66)),
    vario = listOf(Color(0xFF0A5F6C), Color(0xFF1F93A1), Color(0xFF86C9CC), Color(0xFFCBC6C2), Color(0xFFF3A682), Color(0xFFE56954), Color(0xFFB02A45)),
    wind = listOf(Color(0xFFE9E3F2), Color(0xFFCFC3E3), Color(0xFFB0A0D0), Color(0xFF8F78BA), Color(0xFF6F56A3), Color(0xFF4F3A82), Color(0xFF302157)),
    thermal = listOf(Color(0xFFFDE7CF), Color(0xFFFBCDA6), Color(0xFFF6A97C), Color(0xFFEC8260), Color(0xFFD95A4A), Color(0xFFA93640)),
)

object VentiSpace {
    val s1 = 4.dp
    val s2 = 8.dp
    val s3 = 12.dp
    val s4 = 16.dp
    val s5 = 24.dp
    val s6 = 32.dp
    val s7 = 48.dp
    val s8 = 72.dp
}

object VentiRadius {
    val s = 8.dp
    val m = 14.dp
    val l = 24.dp
    val pill = 999.dp
}
