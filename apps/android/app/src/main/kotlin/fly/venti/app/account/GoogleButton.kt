package fly.venti.app.account

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.DeviceFontFamilyName
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fly.venti.app.R
import fly.venti.app.design.LocalDarkAppearance

/**
 * Google's button for signing in with a Google account, drawn by the app
 * itself to Google's "Sign in with Google Branding Guidelines"
 * (https://developers.google.com/identity/branding-guidelines, read
 * 2026-11-21), which allow a custom button that keeps to them.
 *
 * This is the one place in the app whose colours, sizes and font are
 * Google's and not the design pack's tokens, by the owner's word of
 * 2026-11-21. Take nothing here from `Venti.colours` or `VentiSize`, and
 * change a number only to what Google's page says:
 *
 * - Light: fill #FFFFFF, stroke #747775 of 1 px inside, text #1F1F1F.
 *   Dark: fill #131314, stroke #8E918F, text #E3E3E3. The app's light
 *   appearance takes the light one and its dark the dark; the cockpit's
 *   flight theme never shows the button.
 * - 40 high, a pill; 12 before the logo, 10 between logo and text, 12
 *   after the text; the logo 20 high. The button may be scaled, so it is
 *   as wide as the caller makes it, with logo and text in its middle, and
 *   grows with the phone's text size.
 * - Text in Google Sans Medium, 14 on a line of 20. The app does not
 *   bundle Google Sans: it asks the phone for it (`google-sans`, which
 *   Google's own phones have) and takes the phone's own font, Roboto on
 *   most, where there is none.
 * - The "G" is Google's own file, `res/drawable-nodpi/google_g.png`,
 *   unchanged: never redraw, recolour, crop or tint it. Google's page says
 *   its colour and its proportions must stay as they are.
 */
@Composable
fun GoogleButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val look = if (LocalDarkAppearance.current) GoogleLook.Dark else GoogleLook.Light
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = CircleShape,
        color = look.fill,
        contentColor = look.text,
        border = BorderStroke(1.dp, look.stroke),
        // Dimmed as a whole while it cannot be pressed: Google's page names no look for that.
        modifier = modifier.semantics { role = Role.Button }.alpha(if (enabled) 1f else GoogleLook.DISABLED),
    ) {
        Row(
            modifier = Modifier.heightIn(min = 40.dp).padding(horizontal = 12.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // The name of the button is its text: the logo says nothing more.
            Image(
                painterResource(R.drawable.google_g),
                contentDescription = null,
                contentScale = ContentScale.Fit,
                modifier = Modifier.size(20.dp),
            )
            Spacer(Modifier.width(10.dp))
            Text(text, style = GoogleLook.text, textAlign = TextAlign.Center)
        }
    }
}

/** The button's colours and type, as Google's branding guidelines give them. */
private class GoogleLook(val fill: Color, val stroke: Color, val text: Color) {
    companion object {
        val Light = GoogleLook(fill = Color(0xFFFFFFFF), stroke = Color(0xFF747775), text = Color(0xFF1F1F1F))
        val Dark = GoogleLook(fill = Color(0xFF131314), stroke = Color(0xFF8E918F), text = Color(0xFFE3E3E3))

        /** Material's opacity of what cannot be pressed. */
        const val DISABLED = 0.38f

        /** Google Sans Medium 14/20 where the phone has the family, the phone's own font where not. */
        val text = TextStyle(
            fontFamily = FontFamily(Font(DeviceFontFamilyName("google-sans"), FontWeight.Medium)),
            fontWeight = FontWeight.Medium,
            fontSize = 14.sp,
            lineHeight = 20.sp,
        )
    }
}
