package fly.venti.app.flight

import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.R
import fly.venti.app.SealedStore
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.Venti
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.core.LeagueAnswer
import fly.venti.core.LeagueHandover
import fly.venti.core.LeagueRequest
import fly.venti.core.dhvxcUploaded
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/** What the sheet says that is not the core's. The core words the deadline, the signature and the answers. */
object LeagueWords {
    const val TITLE = "Send to a league"
    const val BEFORE = "Before it goes"
    const val SAVE_FILE = "Save the flight file"
    const val OPEN_XCONTEST = "Open XContest’s claim page"
    const val XCONTEST_FOOTER =
        "XContest takes a flight on its own page, from you: save the file, sign in there, choose “Claim " +
            "flight” and upload it. XContest gives other apps no way to do that for you yet."
    const val USER = "User name or e-mail"
    const val PASSWORD = "Upload password"
    const val GLIDER = "Glider, as set up at the DHV-XC"
    const val PUBLISH = "Publish at once"
    const val SEND = "Send to the DHV-XC"
    const val SENDING = "Sending"
    const val DHVXC_FOOTER =
        "The upload password is not the one you sign in with: you set it in your DHV-XC profile. It is kept " +
            "in this phone’s keystore and sent to the DHV-XC only. Without “Publish at once” the " +
            "flight waits among your uploads there, for you to look at and publish."
    const val PROFILE = "Your DHV-XC profile"
    const val RULES = "The DHV-XC’s rules"
    const val ASK_TITLE = "Send this flight to the DHV-XC?"
    const val NOT_REACHED = "The DHV-XC could not be reached"

    /** What the pilot is asked before the flight leaves the phone. */
    fun asking(user: String, publishes: Boolean): String {
        val what = if (publishes) {
            "The DHV-XC publishes it at once if it can."
        } else {
            "It waits among your uploads there until you publish it."
        }
        return "As ${user.trim()}. $what By the DHV-XC’s rules, whoever uploads a flight gives the DHV the " +
            "right to use its file."
    }
}

/** Where the pilot's DHV-XC upload password is kept. */
interface UploadPasswordStore {
    fun read(): String?

    fun save(password: String): Boolean
}

/**
 * The upload password on a phone: sealed under a key in Android's keystore,
 * in a file of the app's own that no backup takes and that does not move
 * to a new phone, as the session's token is. It goes to the DHV-XC only.
 */
class KeystoreUploadPasswordStore(context: Context) : UploadPasswordStore {
    private val sealed = SealedStore(context, file = FILE, alias = "fly.venti.dhvxc", entry = "upload-password")

    override fun read(): String? = sealed.read()

    override fun save(password: String): Boolean = sealed.save(password)

    companion object {
        /** The name of the file the sealed password is in, which the rules for a new phone name. */
        const val FILE = "venti-dhvxc"
    }
}

/** A password that is simply there, or not. Tests use it. */
class MemoryUploadPasswordStore(private var password: String? = null) : UploadPasswordStore {
    override fun read(): String? = password

    override fun save(password: String): Boolean {
        this.password = password
        return true
    }
}

/**
 * A flight on its way to a league. What the leagues take, until when, and
 * what is sent to the DHV-XC is the core's to know; this sends the core's
 * request from the phone, to the DHV-XC and nowhere else.
 */
class Leagues(
    private val logbook: LogbookModel,
    val preferences: Preferences,
    private val passwords: UploadPasswordStore,
    /** Posts a request to the league's own address and brings back the answer. */
    private val post: suspend (LeagueRequest) -> HttpAnswer = { request ->
        Http.send(
            "POST",
            request.url,
            listOf("Content-Type" to request.contentType, "Accept" to "application/json"),
            request.body,
            answerWithinMilliseconds = 60_000,
        )
    },
) {
    /** Venti's time, seconds since 1970, for a league's deadline; 0 while it is unknown. The app sets this. */
    var clock: suspend () -> Long = { System.currentTimeMillis() / 1000 }

    suspend fun handover(id: String): LeagueHandover = logbook.leagueHandover(id, clock())

    /** The password kept from the last upload the DHV-XC took, if any. */
    fun keptPassword(): String = passwords.read().orEmpty()

    /** Whether a flight can be sent with this much filled in. */
    fun canSend(user: String, password: String): Boolean = user.isNotBlank() && password.isNotEmpty()

    /**
     * Sends a flight to the DHV-XC and says what it answered. A password the
     * DHV-XC took is kept; one it refused is not.
     */
    suspend fun sendToDhvxc(id: String, user: String, password: String, glider: String, publishes: Boolean): LeagueAnswer {
        val request = logbook.dhvxcRequest(id, user, password, glider.trim().ifEmpty { null }, publishes)
        val answer = try {
            post(request)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            return LeagueAnswer(taken = false, title = LeagueWords.NOT_REACHED, words = error.pilotMessage)
        }
        val said = dhvxcUploaded(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body)
        if (said.taken) {
            passwords.save(password)
        }
        return said
    }
}

/**
 * "Send to a league", from the button under a flight's profile: what both leagues ask about,
 * XContest's way with the file and its claim page, and the DHV-XC's form.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LeagueSheet(flight: String, leagues: Leagues, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val links = LocalUriHandler.current
    val scope = rememberCoroutineScope()
    val preferences = leagues.preferences
    var handover by remember(flight) { mutableStateOf<LeagueHandover?>(null) }
    var problem by remember(flight) { mutableStateOf<String?>(null) }
    var user by remember { mutableStateOf(preferences.dhvxcUser) }
    var password by remember { mutableStateOf(leagues.keptPassword()) }
    var glider by remember { mutableStateOf(preferences.dhvxcGlider) }
    var publishes by remember { mutableStateOf(preferences.dhvxcPublishes) }
    var isAsking by remember { mutableStateOf(false) }
    var isSending by remember { mutableStateOf(false) }
    var answer by remember { mutableStateOf<LeagueAnswer?>(null) }

    LaunchedEffect(flight) {
        try {
            handover = leagues.handover(flight)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = error.pilotMessage
        }
    }
    // Android's own way to save a file where the pilot chooses.
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val file = handover?.file ?: return@rememberLauncherForActivityResult
        if (uri != null) {
            scope.launch {
                runCatching { context.contentResolver.openOutputStream(uri)?.use { it.write(file.igcText.toByteArray()) } }
                    .onFailure { problem = it.pilotMessage }
            }
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(LeagueWords.TITLE, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("league-close")) { Text(Common.words.done) }
        }
        Box(Modifier.fillMaxHeight()) {
            val current = handover
            when {
                current != null -> Column(
                    Modifier
                        .testTag("league-sheet")
                        .verticalScroll(rememberScrollState())
                        .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
                ) {
                    Before(current)

                    SectionHeader("XContest")
                    Group {
                        row {
                            ActionRow(LeagueWords.SAVE_FILE, MaterialTheme.colorScheme.primary, Modifier.testTag("league-file")) {
                                save.launch(current.file.name)
                            }
                        }
                        row {
                            ActionRow(LeagueWords.OPEN_XCONTEST, MaterialTheme.colorScheme.primary, Modifier.testTag("xcontest-claim")) {
                                links.openUri(current.xcontestClaimUrl)
                            }
                        }
                    }
                    SectionFooter(LeagueWords.XCONTEST_FOOTER)

                    SectionHeader("DHV-XC")
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedTextField(
                            value = user,
                            onValueChange = {
                                user = it
                                preferences.dhvxcUser = it.trim()
                            },
                            label = { Text(LeagueWords.USER) },
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, autoCorrectEnabled = false),
                            modifier = Modifier.fillMaxWidth().testTag("dhvxc-user").semantics { contentType = ContentType.Username },
                        )
                        OutlinedTextField(
                            value = password,
                            onValueChange = { password = it },
                            label = { Text(LeagueWords.PASSWORD) },
                            singleLine = true,
                            visualTransformation = PasswordVisualTransformation(),
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
                            modifier = Modifier.fillMaxWidth().testTag("dhvxc-password").semantics { contentType = ContentType.Password },
                        )
                        OutlinedTextField(
                            value = glider,
                            onValueChange = {
                                glider = it
                                preferences.dhvxcGlider = it.trim()
                            },
                            label = { Text(LeagueWords.GLIDER) },
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false),
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(LeagueWords.PUBLISH, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                            Switch(
                                checked = publishes,
                                onCheckedChange = {
                                    publishes = it
                                    preferences.dhvxcPublishes = it
                                },
                                modifier = Modifier.testTag("dhvxc-publish"),
                            )
                        }
                        Button(
                            onClick = { isAsking = true },
                            enabled = !isSending && leagues.canSend(user, password),
                            modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("dhvxc-send"),
                        ) {
                            if (isSending) {
                                CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                                Spacer(Modifier.width(8.dp))
                                Text(LeagueWords.SENDING)
                            } else {
                                Icon(painterResource(R.drawable.ic_send), contentDescription = null, modifier = Modifier.size(18.dp))
                                Spacer(Modifier.width(8.dp))
                                Text(LeagueWords.SEND)
                            }
                        }
                    }
                    SectionFooter(LeagueWords.DHVXC_FOOTER)
                    TextButton(onClick = { links.openUri(current.dhvxcProfileUrl) }) { Text(LeagueWords.PROFILE) }
                    TextButton(onClick = { links.openUri(current.dhvxcRulesUrl) }) { Text(LeagueWords.RULES) }
                }
                problem != null -> Text(
                    checkNotNull(problem),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(24.dp),
                )
                else -> CircularProgressIndicator(Modifier.align(Alignment.TopCenter).padding(top = 32.dp))
            }
        }
    }

    if (isAsking) {
        AlertDialog(
            onDismissRequest = { isAsking = false },
            title = { Text(LeagueWords.ASK_TITLE) },
            text = { Text(LeagueWords.asking(user, publishes)) },
            confirmButton = {
                TextButton(
                    onClick = {
                        isAsking = false
                        isSending = true
                        scope.launch {
                            try {
                                answer = leagues.sendToDhvxc(flight, user, password, glider, publishes)
                            } finally {
                                isSending = false
                            }
                        }
                    },
                ) { Text("Send") }
            },
            dismissButton = { TextButton(onClick = { isAsking = false }) { Text(Common.words.cancel) } },
        )
    }

    answer?.let { said ->
        AlertDialog(
            onDismissRequest = { answer = null },
            title = { Text(said.title) },
            text = { Text(said.words) },
            confirmButton = { TextButton(onClick = { answer = null }) { Text(Common.words.ok) } },
        )
    }
}

/** What both leagues ask about: until when they take the flight, and its signature. Status is a sign, a word and a colour. */
@Composable
private fun Before(handover: LeagueHandover) {
    val colours = Venti.colours
    SectionHeader(LeagueWords.BEFORE)
    Group {
        row {
            Line(
                icon = if (handover.isLate) R.drawable.venti_caution else R.drawable.ic_calendar_month,
                tint = if (handover.isLate) colours.caution else colours.primary,
                text = handover.deadline,
                tag = "league-deadline",
            )
        }
        row {
            Line(
                icon = if (handover.isSigned) R.drawable.ic_verified else R.drawable.venti_caution,
                tint = if (handover.isSigned) colours.good else colours.caution,
                text = handover.signature,
                tag = "league-signature",
            )
        }
    }
}

@Composable
private fun Line(icon: Int, tint: androidx.compose.ui.graphics.Color, text: String, tag: String) {
    Row(Modifier.fillMaxWidth().testTag(tag).padding(horizontal = 16.dp, vertical = 12.dp)) {
        Icon(painterResource(icon), contentDescription = null, tint = tint, modifier = Modifier.size(22.dp))
        Spacer(Modifier.width(12.dp))
        Text(text, style = MaterialTheme.typography.bodyMedium)
    }
}
