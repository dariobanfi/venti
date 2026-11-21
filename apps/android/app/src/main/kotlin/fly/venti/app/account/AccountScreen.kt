package fly.venti.app.account

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.alerts.AlertWords
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.WindServer
import fly.venti.core.AccountView
import fly.venti.core.GoogleSignUp
import fly.venti.core.Registration
import fly.venti.core.googleWords
import fly.venti.core.syncSummaryInWords
import fly.venti.core.tokenForm
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.net.URI

/**
 * The pilot's account on a Venti server: sign in or create one, see how the
 * logbook and the server stand, sign out, delete.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AccountScreen(
    account: AccountModel,
    /** The server set in the settings: an account is made and signed in to there. */
    wind: WindModel,
    /** Where requests run, so that one carries on when the pilot leaves the screen. */
    scope: CoroutineScope,
    /** Opens "Alerts", the pilot's alarms. */
    onAlerts: () -> Unit,
    onBack: () -> Unit,
) {
    // What the server says of the account now, asked when its page opens:
    // its name, and whether it is one of Venti's partners for flying sites.
    LaunchedEffect(account) {
        scope.launch { account.refresh() }
    }
    // Whether the server offers Google, and takes accounts by invite only.
    LaunchedEffect(wind) { wind.readStatus() }
    // "Programs with access" is a page of the account's, laid over it.
    var showsTokens by rememberSaveable { mutableStateOf(false) }
    if (showsTokens && account.isSignedIn) {
        TokensScreen(account, onBack = { showsTokens = false })
        return
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Account") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                // The form stays in reach above the keyboard.
                .consumeWindowInsets(padding)
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 24.dp),
        ) {
            val signedIn = account.account
            val signUp = account.googleSignUp
            when {
                signedIn != null -> SignedIn(signedIn, account, wind, scope, onTokens = { showsTokens = true }, onAlerts = onAlerts)
                signUp != null -> ChooseHandle(account, signUp.second, wind.registration, scope)
                else -> SignInForm(account, (wind.server as? WindServer.Usable)?.url, wind.googleSignIn, scope)
            }
        }
    }

    account.problem?.let { problem ->
        AlertDialog(
            onDismissRequest = { account.problem = null },
            title = { Text("Account") },
            text = { Text(problem) },
            confirmButton = { TextButton(onClick = { account.problem = null }) { Text("OK") } },
        )
    }
}

/** A server's name as a pilot knows it: without `https://` and what follows the name. */
private fun host(server: String): String = runCatching { URI(server).host }.getOrNull() ?: server

/**
 * Signing in: "Continue with Google" first where the server offers it, and
 * the handle and password form behind "Sign in with a handle and password";
 * the form alone where the server offers no Google.
 */
@Composable
private fun SignInForm(
    account: AccountModel,
    /** The server to sign in to, or `null` if the settings name none the app can ask. */
    server: String?,
    /** Whether the server offers signing in with Google. */
    offersGoogle: Boolean,
    scope: CoroutineScope,
) {
    val context = LocalContext.current
    val google = remember { googleWords() }
    var showsPassword by rememberSaveable { mutableStateOf(false) }
    var isNew by rememberSaveable { mutableStateOf(false) }
    var handle by rememberSaveable { mutableStateOf("") }
    var displayName by rememberSaveable { mutableStateOf("") }
    // Not kept when Android puts the screen away: a password is written nowhere.
    var password by remember { mutableStateOf("") }
    var invite by rememberSaveable { mutableStateOf("") }
    val plain = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)

    Text(
        "With an account, your flights are copied to Venti. They appear on your other devices, " +
            "and you can share one with a link. A flight stays private until you share it.",
        style = MaterialTheme.typography.bodyLarge,
        modifier = Modifier.padding(horizontal = 4.dp, vertical = 8.dp),
    )

    if (server == null) {
        Text(
            // Only a test is without Venti; the app always has it.
            "Venti could not be reached.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 4.dp, vertical = 8.dp),
        )
        return
    }

    SectionHeader(host(server))
    if (offersGoogle) {
        // The pack's primary button with the core's words. Google's own
        // sheet, with its branding, opens over the screen.
        Button(
            onClick = { scope.launch { account.signInWithGoogle(server, account.googleIdTokens(context, server)) } },
            enabled = !account.isWorking,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("account-google"),
        ) {
            Text(google.continueWithGoogle)
        }
        if (!showsPassword) {
            Spacer(Modifier.height(VentiSpace.s2))
            TextButton(
                onClick = { showsPassword = true },
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("account-with-password"),
            ) {
                Text(google.withPassword)
            }
            return
        }
        Spacer(Modifier.height(VentiSpace.s5))
    }
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        SegmentedButton(
            selected = !isNew,
            onClick = { isNew = false },
            shape = SegmentedButtonDefaults.itemShape(0, 2),
            modifier = Modifier.testTag("account-existing"),
        ) {
            Text("Sign in")
        }
        SegmentedButton(
            selected = isNew,
            onClick = { isNew = true },
            shape = SegmentedButtonDefaults.itemShape(1, 2),
            modifier = Modifier.testTag("account-new"),
        ) {
            Text("New account")
        }
    }
    Spacer(Modifier.height(12.dp))
    // A password manager is told which field is which.
    OutlinedTextField(
        value = handle,
        onValueChange = { handle = it },
        label = { Text("Handle") },
        singleLine = true,
        keyboardOptions = plain,
        modifier = Modifier
            .fillMaxWidth()
            .testTag("account-handle")
            .semantics { contentType = if (isNew) ContentType.NewUsername else ContentType.Username },
    )
    if (isNew) {
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = displayName,
            onValueChange = { displayName = it },
            label = { Text("Name shown to others") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false),
            modifier = Modifier.fillMaxWidth().testTag("account-name"),
        )
    }
    Spacer(Modifier.height(8.dp))
    OutlinedTextField(
        value = password,
        onValueChange = { password = it },
        label = { Text("Password") },
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.None,
            autoCorrectEnabled = false,
            keyboardType = KeyboardType.Password,
        ),
        modifier = Modifier
            .fillMaxWidth()
            .testTag("account-password")
            .semantics { contentType = if (isNew) ContentType.NewPassword else ContentType.Password },
    )
    if (isNew) {
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = invite,
            onValueChange = { invite = it },
            label = { Text("Invite code") },
            singleLine = true,
            keyboardOptions = plain,
            modifier = Modifier.fillMaxWidth().testTag("account-invite"),
        )
        SectionFooter(
            "A handle is 3 to 20 letters, digits or underscores. A password needs at least 15 characters; a " +
                "few words in a row are easy to remember. A forgotten password cannot be recovered: Venti " +
                "knows no email address. Handle and name are public once you share a flight.",
        )
    }
    Spacer(Modifier.height(20.dp))
    Button(
        onClick = {
            scope.launch {
                if (isNew) {
                    account.createAccount(server, handle, displayName, password, invite)
                } else {
                    account.signIn(server, handle, password)
                }
                if (account.account != null) {
                    password = ""
                }
            }
        },
        enabled = !account.isWorking && handle.isNotEmpty() && password.isNotEmpty(),
        modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("account-submit"),
    ) {
        Text(if (isNew) "Create account" else "Sign in")
    }
}

/**
 * The handle step after a first sign-in with Google: the server's free
 * handle filled in with the core's line saying where it is from, the name
 * from Google, an invite code where the server takes accounts by invite,
 * and "Create account".
 */
@Composable
private fun ChooseHandle(
    account: AccountModel,
    signUp: GoogleSignUp,
    /** Who may create an account on the server; `null` when it has not said, which may mean an invite. */
    registration: Registration?,
    scope: CoroutineScope,
) {
    val google = remember { googleWords() }
    var handle by rememberSaveable { mutableStateOf(signUp.suggestedHandle.orEmpty()) }
    var displayName by rememberSaveable { mutableStateOf(signUp.name.orEmpty()) }
    var invite by rememberSaveable { mutableStateOf("") }
    val plain = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)

    Text(
        google.chooseHandle,
        style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier
            .padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2)
            .testTag("account-choose-handle")
            .semantics { heading() },
    )
    Text(
        google.chooseHandleLine,
        style = MaterialTheme.typography.bodyLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2),
    )
    Spacer(Modifier.height(VentiSpace.s3))
    OutlinedTextField(
        value = handle,
        onValueChange = { handle = it },
        label = { Text("Handle") },
        singleLine = true,
        keyboardOptions = plain,
        // The core's line holds for the suggestion as it was made.
        supportingText = signUp.suggestionLine?.takeIf { handle == signUp.suggestedHandle }?.let { line -> { Text(line) } },
        modifier = Modifier
            .fillMaxWidth()
            .testTag("account-google-handle")
            .semantics { contentType = ContentType.NewUsername },
    )
    Spacer(Modifier.height(VentiSpace.s2))
    OutlinedTextField(
        value = displayName,
        onValueChange = { displayName = it },
        label = { Text("Name shown to others") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false),
        modifier = Modifier.fillMaxWidth().testTag("account-google-name"),
    )
    if (registration == null || registration == Registration.INVITE) {
        Spacer(Modifier.height(VentiSpace.s2))
        OutlinedTextField(
            value = invite,
            onValueChange = { invite = it },
            label = { Text("Invite code") },
            singleLine = true,
            keyboardOptions = plain,
            modifier = Modifier.fillMaxWidth().testTag("account-google-invite"),
        )
    }
    Spacer(Modifier.height(VentiSpace.s5))
    Button(
        onClick = { scope.launch { account.createGoogleAccount(handle, displayName, invite) } },
        enabled = !account.isWorking && handle.isNotEmpty(),
        modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("account-google-create"),
    ) {
        Text(google.createAccount)
    }
    Spacer(Modifier.height(VentiSpace.s2))
    TextButton(
        onClick = account::cancelGoogleSignUp,
        enabled = !account.isWorking,
        modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch),
    ) {
        Text("Cancel")
    }
}

/** The account, and how the logbook and the server stand. */
@Composable
private fun SignedIn(
    signedIn: AccountView,
    account: AccountModel,
    wind: WindModel,
    scope: CoroutineScope,
    onTokens: () -> Unit,
    onAlerts: () -> Unit,
) {
    val context = LocalContext.current
    val google = remember { googleWords() }
    var isConfirmingSignOut by remember { mutableStateOf(false) }
    var isDeleting by remember { mutableStateOf(false) }

    Group {
        row { ValueRow("Handle", "@${signedIn.handle}") }
        row { ValueRow("Name", signedIn.displayName) }
        row { ValueRow("Server", host(signedIn.server)) }
        // The Google account it signs in with, shown to the pilot alone.
        signedIn.googleEmail?.let { email ->
            row { ValueRow("Google", email, Modifier.testTag("account-google-email")) }
        }
    }
    if (account.isSignedIn && (signedIn.googleEmail != null || wind.googleSignIn)) {
        Spacer(Modifier.height(VentiSpace.s3))
        Group {
            row {
                if (signedIn.googleEmail == null) {
                    ActionRow(
                        google.linkGoogle,
                        MaterialTheme.colorScheme.primary,
                        Modifier.testTag("account-link-google"),
                        enabled = !account.isWorking,
                    ) {
                        scope.launch { account.linkGoogle(account.googleIdTokens(context, signedIn.server)) }
                    }
                } else {
                    ActionRow(
                        google.unlinkGoogle,
                        MaterialTheme.colorScheme.primary,
                        Modifier.testTag("account-unlink-google"),
                        enabled = !account.isWorking,
                    ) {
                        scope.launch { account.unlinkGoogle() }
                    }
                }
            }
        }
    }

    SectionHeader("Logbook")
    Group {
        if (account.needsSignIn) {
            row {
                Row(Modifier.padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.Top) {
                    Icon(painterResource(R.drawable.venti_caution), contentDescription = null)
                    Spacer(Modifier.width(16.dp))
                    Text(
                        "This phone was signed out. Sign out here and sign in again.",
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
            }
        }
        row { ValueRow("Flights on Venti", Format.count(signedIn.flightsOnServer.toLong())) }
        if (signedIn.flightsToSend > 0u) {
            row { ValueRow("Waiting to be sent", Format.count(signedIn.flightsToSend.toLong())) }
        }
        signedIn.lastSyncUnixS?.let { last ->
            row { ValueRow("In step since", "${Format.dayInYear(last)}, ${Format.time(last)}") }
        }
        // What the last sync did, in the core's words, as on the web.
        account.lastSync?.let { summary ->
            row { ValueRow("Last sync", syncSummaryInWords(summary), Modifier.testTag("account-last-sync")) }
        }
        row {
            if (account.isWorking) {
                Row(
                    modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    Text(
                        "Working",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            } else {
                ActionRow("Sync now", MaterialTheme.colorScheme.primary, enabled = !account.needsSignIn) {
                    scope.launch { account.sync(force = true, quietly = false) }
                }
            }
        }
    }
    SectionFooter(
        "Venti syncs by itself when you open the app and after a flight. Your flights stay private on the " +
            "Venti until you share one.",
    )

    if (account.isSignedIn) {
        // What other programs may do for the pilot: the core's name for it.
        Spacer(Modifier.height(VentiSpace.s5))
        Group {
            row {
                LinkRow(tokenForm().title, R.drawable.ic_chevron_right, Modifier.testTag("account-tokens"), onClick = onTokens)
            }
            row {
                LinkRow(
                    AlertWords.core.title,
                    R.drawable.ic_chevron_right,
                    Modifier.testTag("account-alerts"),
                    leading = R.drawable.ic_notifications,
                    onClick = onAlerts,
                )
            }
        }
    }

    Spacer(Modifier.height(24.dp))
    Group {
        row {
            ActionRow(
                "Sign out",
                MaterialTheme.colorScheme.primary,
                Modifier.testTag("account-sign-out"),
                enabled = !account.isWorking,
            ) {
                isConfirmingSignOut = true
            }
        }
        row {
            ActionRow(
                "Delete account",
                MaterialTheme.colorScheme.error,
                Modifier.testTag("account-delete"),
                enabled = !account.isWorking,
            ) {
                isDeleting = true
            }
        }
    }

    if (isConfirmingSignOut) {
        AlertDialog(
            onDismissRequest = { isConfirmingSignOut = false },
            title = { Text("Sign out?") },
            text = { Text("Your flights stay on this phone and on Venti.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingSignOut = false
                        scope.launch { account.signOut() }
                    },
                    modifier = Modifier.testTag("account-sign-out-confirm"),
                ) {
                    Text("Sign out")
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingSignOut = false }) { Text("Cancel") } },
        )
    }
    if (isDeleting) {
        DeleteAccountDialog(
            onDelete = { password ->
                isDeleting = false
                scope.launch { account.deleteAccount(password) }
            },
            // An account made with Google has no password: a fresh sign-in
            // with its Google account stands in for it.
            withGoogle = if (signedIn.googleEmail != null) {
                {
                    isDeleting = false
                    scope.launch { account.deleteAccountWithGoogle(account.googleIdTokens(context, signedIn.server)) }
                }
            } else {
                null
            },
            onDismiss = { isDeleting = false },
        )
    }
}

/** Asks for the password before the account is deleted: a token alone does not do that. */
@Composable
private fun DeleteAccountDialog(onDelete: (String) -> Unit, withGoogle: (() -> Unit)?, onDismiss: () -> Unit) {
    var password by remember { mutableStateOf("") }
    val google = remember { googleWords() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Delete your account?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(
                    "The account and every flight on Venti are deleted at once and for good, and your " +
                        "shared links stop working. The flights on this phone stay. Enter your password to go on.",
                )
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    label = { Text("Password") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Password,
                    ),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("account-delete-password")
                        .semantics { contentType = ContentType.Password },
                )
                if (withGoogle != null) {
                    TextButton(
                        onClick = withGoogle,
                        colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                        modifier = Modifier.fillMaxWidth().testTag("account-delete-google"),
                    ) {
                        Text(google.continueWithGoogle)
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onDelete(password) },
                enabled = password.isNotEmpty(),
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                modifier = Modifier.testTag("account-delete-confirm"),
            ) {
                Text("Delete account")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
