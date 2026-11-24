package fly.venti.app.account

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.PersistableBundle
import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.StatusChip
import fly.venti.app.design.StatusKind
import fly.venti.app.design.ValueRow
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.TokenCreated
import fly.venti.core.TokenForm
import fly.venti.core.TokenScope
import fly.venti.core.TokenView
import fly.venti.core.tokenWithdrawal
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/**
 * "Programs with access", a page of the account: the tokens the pilot has
 * given to other programs, the form that gives one, the one moment its
 * secret is shown, and the question before one is withdrawn. Every word
 * about what a token is and may do, and every refusal, is the core's.
 *
 * The secret is held by this screen alone, while its page shows: it is not
 * in the screen's saved state, so Android putting the screen away loses
 * it, and it is let go when the pilot leaves the page.
 */
@Composable
fun TokensScreen(account: AccountModel, onBack: () -> Unit) {
    val model = remember(account) { TokensModel(account) }
    val form = model.form
    val scope = rememberCoroutineScope()
    var listed by remember { mutableStateOf<TokensModel.Listed?>(null) }
    // Counted up to ask for the list again.
    var asked by remember { mutableIntStateOf(0) }
    var isGiving by rememberSaveable { mutableStateOf(false) }
    // Never saved: see above.
    var made by remember { mutableStateOf<TokenCreated?>(null) }
    var withdrawing by remember { mutableStateOf<TokenView?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(asked) {
        listed = model.list()
    }

    val shown = made
    when {
        shown != null -> SecretPage(form, shown) {
            made = null
            isGiving = false
            asked += 1
        }
        isGiving -> FormPage(
            model,
            onMade = { made = it },
            onBack = { isGiving = false },
        )
        else -> ListPage(
            form,
            listed,
            onGive = { isGiving = true },
            onWithdraw = { withdrawing = it },
            onAgain = {
                listed = null
                asked += 1
            },
            onBack = onBack,
        )
    }

    withdrawing?.let { token ->
        val words = model.withdrawal(token)
        AlertDialog(
            onDismissRequest = { withdrawing = null },
            title = { Text(words.title) },
            text = { Text(words.message) },
            confirmButton = {
                TextButton(
                    onClick = {
                        withdrawing = null
                        scope.launch {
                            problem = model.withdraw(token)
                            asked += 1
                        }
                    },
                    modifier = Modifier.testTag("token-withdraw-confirm"),
                ) {
                    Text(words.confirm, color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { withdrawing = null }) { Text(words.cancel) } },
        )
    }
    problem?.let { words -> Refusal(form, words) { problem = null } }
}

/** What the server or the core refused, in its words. */
@Composable
private fun Refusal(form: TokenForm, words: String, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(form.title) },
        text = { Text(words, Modifier.testTag("token-problem")) },
        confirmButton = { TextButton(onClick = onDismiss) { Text(Common.words.ok) } },
    )
}

/** A page of the account with its name over it; without `onBack` it has no way back but its own button. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Page(title: String, onBack: (() -> Unit)?, content: @Composable () -> Unit) {
    // The page lies over the account's: one step back leaves it, not the account.
    BackHandler { onBack?.invoke() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                        }
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
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s5),
        ) {
            content()
        }
    }
}

/** The core's intro, the tokens or the core's line that there are none, what no program can do, and "Give access". */
@Composable
private fun ListPage(
    form: TokenForm,
    listed: TokensModel.Listed?,
    onGive: () -> Unit,
    onWithdraw: (TokenView) -> Unit,
    onAgain: () -> Unit,
    onBack: () -> Unit,
) {
    Page(form.title, onBack) {
        Text(
            form.intro,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2),
        )
        Spacer(Modifier.height(VentiSpace.s2))
        when (listed) {
            null -> Row(
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).padding(horizontal = VentiSpace.s1),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(Modifier.size(VentiSize.icon))
            }
            is TokensModel.Listed.Refused -> {
                Text(
                    listed.words,
                    style = MaterialTheme.typography.bodyLarge,
                    color = Venti.colours.textSecondary,
                    modifier = Modifier.padding(horizontal = VentiSpace.s1).testTag("tokens-problem"),
                )
                TextButton(onClick = onAgain) { Text(Common.words.tryAgain) }
            }
            is TokensModel.Listed.Tokens -> if (listed.tokens.isEmpty()) {
                Text(
                    form.empty,
                    style = MaterialTheme.typography.bodyLarge,
                    color = Venti.colours.textSecondary,
                    modifier = Modifier.padding(horizontal = VentiSpace.s1).testTag("tokens-empty"),
                )
            } else {
                // Each its own group, with the core's `confirm` under it, as on the iPhone.
                Column(Modifier.testTag("tokens"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                    for (token in listed.tokens) {
                        Group {
                            row { TokenRow(token) }
                            row {
                                ActionRow(
                                    tokenWithdrawal(token.name).confirm,
                                    MaterialTheme.colorScheme.error,
                                    Modifier.testTag("token-withdraw"),
                                    onClick = { onWithdraw(token) },
                                )
                            }
                        }
                    }
                }
            }
        }
        SectionFooter(form.never)
        Spacer(Modifier.height(VentiSpace.s5))
        Button(
            onClick = onGive,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("tokens-give"),
        ) {
            Text(form.submit)
        }
    }
}

/** One token: its name, what it may do, when it was made, last used and ends. */
@Composable
private fun TokenRow(token: TokenView) {
    val detail = TokenWords.detail(token)
    Column(Modifier.fillMaxWidth().testTag("token")) {
        Column(
            // One element for a screen reader: the name, then the rest.
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = VentiSize.touch)
                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                .clearAndSetSemantics {
                    contentDescription = token.name
                    stateDescription = detail
                },
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        ) {
            Text(token.name, style = MaterialTheme.typography.bodyLarge)
            if (token.hasEnded) {
                // Marked with a sign and a word, not by colour alone.
                StatusChip(StatusKind.Caution, TokenWords.ENDED)
            }
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * The form that gives a program access: its name, what it may do, an end
 * if the pilot wants one, and the password. What is wrong with any of it
 * is said in the core's words, the name's under its field.
 */
@Composable
private fun FormPage(model: TokensModel, onMade: (TokenCreated) -> Unit, onBack: () -> Unit) {
    val form = model.form
    val scope = rememberCoroutineScope()
    var name by rememberSaveable { mutableStateOf("") }
    var nameFault by remember { mutableStateOf<String?>(null) }
    var chosen by rememberSaveable { mutableStateOf(emptyList<String>()) }
    // A date picker's moment: midnight UTC of the day the access ends on.
    var endsOn by rememberSaveable { mutableStateOf<Long?>(null) }
    var isChoosingEnd by remember { mutableStateOf(false) }
    // Not kept when Android puts the screen away: a password is written nowhere.
    var password by remember { mutableStateOf("") }
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }

    Page(form.submit, onBack) {
        OutlinedTextField(
            value = name,
            onValueChange = {
                name = it
                nameFault = null
            },
            label = { Text(form.nameLabel) },
            placeholder = { Text(form.nameHint) },
            singleLine = true,
            isError = nameFault != null,
            supportingText = nameFault?.let { fault -> { Text(fault) } },
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, autoCorrectEnabled = false),
            modifier = Modifier.fillMaxWidth().padding(top = VentiSpace.s2).testTag("token-name"),
        )

        SectionHeader(form.scopesTitle)
        Group {
            for (allowed in form.scopes) {
                row {
                    ScopeRow(allowed, isOn = allowed.scope in chosen) { isOn ->
                        chosen = if (isOn) chosen + allowed.scope else chosen - allowed.scope
                    }
                }
            }
        }

        Spacer(Modifier.height(VentiSpace.s5))
        Group {
            row {
                ValueRow(
                    form.endsLabel,
                    endsOn?.let(TokenWords::day) ?: TokenWords.NO_END,
                    Modifier.testTag("token-ends"),
                    onClick = { isChoosingEnd = true },
                )
            }
            if (endsOn != null) {
                row { ActionRow(TokenWords.NO_END, MaterialTheme.colorScheme.primary) { endsOn = null } }
            }
        }
        SectionFooter(form.endsHint)

        Spacer(Modifier.height(VentiSpace.s5))
        OutlinedTextField(
            value = password,
            onValueChange = { password = it },
            label = { Text(form.passwordLabel) },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Password,
            ),
            modifier = Modifier
                .fillMaxWidth()
                .testTag("token-password")
                .semantics { contentType = ContentType.Password },
        )
        SectionFooter(form.passwordReason)

        Spacer(Modifier.height(VentiSpace.s5))
        Button(
            onClick = {
                nameFault = model.nameFault(name)
                if (nameFault != null) return@Button
                isSending = true
                scope.launch {
                    val given = model.give(name, chosen, endsOn?.let(TokenWords::endOfDay), password)
                    isSending = false
                    when (given) {
                        is TokensModel.Given.Made -> {
                            password = ""
                            onMade(given.created)
                        }
                        is TokensModel.Given.Refused -> problem = given.words
                    }
                }
            },
            enabled = !isSending,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("token-submit"),
        ) {
            Text(form.submit)
        }
    }

    if (isChoosingEnd) {
        EndChoice(endsOn, onChoose = { endsOn = it }, onDismiss = { isChoosingEnd = false })
    }
    problem?.let { words -> Refusal(form, words) { problem = null } }
}

/** One thing a program can be allowed, to switch on: the core's title and its line of what that opens. */
@Composable
private fun ScopeRow(allowed: TokenScope, isOn: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .toggleable(value = isOn, role = Role.Switch, onValueChange = onChange)
            .testTag("token-scope-${allowed.scope}")
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(allowed.title, style = MaterialTheme.typography.bodyLarge)
            Text(
                allowed.line,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(VentiSpace.s3))
        Switch(checked = isOn, onCheckedChange = null)
    }
}

/** The day the access ends on, from Android's calendar: a day that has not begun. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EndChoice(endsOn: Long?, onChoose: (Long) -> Unit, onDismiss: () -> Unit) {
    val now = remember { System.currentTimeMillis() / 1000 }
    val picker = rememberDatePickerState(
        initialSelectedDateMillis = endsOn,
        selectableDates = object : SelectableDates {
            override fun isSelectableDate(utcTimeMillis: Long): Boolean = TokenWords.canEndOn(utcTimeMillis, now)
        },
    )
    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = {
                    picker.selectedDateMillis?.let(onChoose)
                    onDismiss()
                },
            ) { Text(Common.words.ok) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(Common.words.cancel) } },
    ) {
        DatePicker(picker)
    }
}

/**
 * The secret, the one time it is shown, with the core's words beside it,
 * the button that copies it and the one that leaves. There is no other way
 * off the page: a step back does nothing here.
 */
@Composable
private fun SecretPage(form: TokenForm, created: TokenCreated, onDone: () -> Unit) {
    val context = LocalContext.current
    var isCopied by remember { mutableStateOf(false) }
    Page(form.title, onBack = null) {
        Text(
            created.token.name,
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2),
        )
        Row(
            modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2),
            horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            Icon(painterResource(R.drawable.venti_caution), contentDescription = null, tint = Venti.colours.caution)
            Text(created.shownOnce, style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.height(VentiSpace.s2))
        Group {
            row {
                Text(
                    created.secret,
                    style = MaterialTheme.typography.bodyLarge,
                    fontFamily = FontFamily.Monospace,
                    modifier = Modifier.padding(VentiSpace.s4).testTag("token-secret"),
                )
            }
        }
        Spacer(Modifier.height(VentiSpace.s4))
        Button(
            onClick = {
                copySecret(context, created.secret)
                isCopied = true
            },
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("token-copy"),
        ) {
            Text(created.copy)
        }
        if (isCopied) {
            Row(
                modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2).testTag("token-copied"),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = Venti.colours.textSecondary)
                Text(TokenWords.COPIED, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            }
        }
        Text(
            created.howToUse,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s3),
        )
        Spacer(Modifier.height(VentiSpace.s4))
        OutlinedButton(
            onClick = onDone,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("token-done"),
        ) {
            Text(created.done)
        }
    }
}

/**
 * Puts a token's secret on the clipboard, marked as sensitive, so that
 * Android does not show it in the clipboard's preview or to the keyboard's
 * suggestions. It is written nowhere else.
 */
private fun copySecret(context: Context, secret: String) {
    val clip = ClipData.newPlainText("Venti token", secret)
    // The key's name is the platform's, also on versions older than the
    // one that named it in a constant (Android 13).
    clip.description.extras = PersistableBundle().apply { putBoolean(IS_SENSITIVE, true) }
    context.getSystemService(ClipboardManager::class.java)?.setPrimaryClip(clip)
}

/** `ClipDescription.EXTRA_IS_SENSITIVE`, which Android names from version 13 on and asks older versions to write out. */
private const val IS_SENSITIVE = "android.content.extra.IS_SENSITIVE"
