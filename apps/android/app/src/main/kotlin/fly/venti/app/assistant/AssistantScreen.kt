package fly.venti.app.assistant

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
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
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.core.TranscriptEntry
import fly.venti.core.assistantModels

/**
 * The assistant: questions about the pilot's own flying, answered by a
 * model on the pilot's own account.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AssistantScreen(
    chat: AssistantChat,
    /** Whether a Venti server is set: the assistant can then look up live wind. */
    hasWind: Boolean,
    bars: PaddingValues,
) {
    Scaffold(
        modifier = Modifier
            .padding(bottom = bars.calculateBottomPadding())
            // The tab bar's room is taken; the keyboard asks only for the rest.
            .consumeWindowInsets(PaddingValues(bottom = bars.calculateBottomPadding()))
            .imePadding(),
        topBar = {
            TopAppBar(
                title = { Text("Assistant") },
                actions = {
                    if (chat.hasKey) {
                        Options(chat)
                    }
                },
            )
        },
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            if (chat.hasKey) Conversation(chat, hasWind) else KeyEntry(chat)
        }
    }

    chat.problem?.let { problem ->
        AlertDialog(
            onDismissRequest = { chat.problem = null },
            title = { Text("Assistant") },
            text = { Text(problem) },
            confirmButton = { TextButton(onClick = { chat.problem = null }) { Text("OK") } },
        )
    }
}

/** The menu in the corner: which model, a new conversation, removing the key. */
@Composable
private fun Options(chat: AssistantChat) {
    var isShowing by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { isShowing = true }) {
            Icon(painterResource(R.drawable.ic_more_vert), contentDescription = "Assistant options")
        }
        DropdownMenu(expanded = isShowing, onDismissRequest = { isShowing = false }) {
            for (model in assistantModels()) {
                DropdownMenuItem(
                    text = {
                        Column {
                            Text(model.name)
                            Text(
                                model.note,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    },
                    leadingIcon = {
                        // The model in use is ticked.
                        if (model.id == chat.model) {
                            Icon(painterResource(R.drawable.ic_check), contentDescription = "In use")
                        } else {
                            Spacer(Modifier.size(24.dp))
                        }
                    },
                    onClick = {
                        isShowing = false
                        chat.use(model.id)
                    },
                )
            }
            HorizontalDivider()
            DropdownMenuItem(
                text = { Text("New conversation") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_edit_square), contentDescription = null) },
                onClick = {
                    isShowing = false
                    chat.startAgain()
                },
            )
            DropdownMenuItem(
                text = { Text("Remove my key") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_delete), contentDescription = null) },
                onClick = {
                    isShowing = false
                    chat.removeKey()
                },
            )
        }
    }
}

@Composable
private fun Conversation(chat: AssistantChat, hasWind: Boolean) {
    var draft by rememberSaveable { mutableStateOf("") }
    val list = rememberLazyListState()
    val pending = chat.pending

    fun ask(question: String) {
        draft = ""
        chat.ask(question)
    }

    // The newest words stay in view.
    LaunchedEffect(chat.entries.size, pending) {
        val last = list.layoutInfo.totalItemsCount - 1
        if (last >= 0) {
            list.animateScrollToItem(last)
        }
    }

    Column(Modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.weight(1f).fillMaxWidth(),
            state = list,
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            if (chat.entries.isEmpty() && !chat.isWorking) {
                item { Suggestions(hasWind, ::ask) }
            }
            itemsIndexed(chat.entries) { _, entry ->
                Bubble(entry)
            }
            if (pending != null) {
                item { Bubble(TranscriptEntry(fromPilot = true, text = pending, lookedUp = emptyList())) }
                item {
                    CircularProgressIndicator(
                        modifier = Modifier
                            .padding(start = 4.dp)
                            .size(22.dp)
                            .semantics { contentDescription = "The assistant is answering" },
                        strokeWidth = 2.5.dp,
                    )
                }
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.weight(1f),
                placeholder = { Text("Ask about your flying") },
                maxLines = 5,
                shape = MaterialTheme.shapes.large,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            )
            Spacer(Modifier.width(8.dp))
            FilledIconButton(
                onClick = { ask(draft) },
                enabled = draft.isNotBlank() && !chat.isWorking,
                modifier = Modifier.padding(bottom = 4.dp).size(48.dp),
            ) {
                Icon(painterResource(R.drawable.ic_arrow_upward), contentDescription = "Send")
            }
        }
    }
}

/** What the pilot or the assistant said. */
@Composable
private fun Bubble(entry: TranscriptEntry) {
    val fromPilot = entry.fromPilot
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = if (fromPilot) 40.dp else 0.dp, end = if (fromPilot) 0.dp else 40.dp),
        horizontalAlignment = if (fromPilot) Alignment.End else Alignment.Start,
    ) {
        Surface(
            shape = MaterialTheme.shapes.medium,
            color = if (fromPilot) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHigh,
            contentColor = if (fromPilot) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
        ) {
            SelectionContainer {
                Text(
                    formatted(entry.text),
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }
        if (entry.lookedUp.isNotEmpty()) {
            // Where the answer's numbers came from.
            Row(
                modifier = Modifier.padding(start = 6.dp, end = 6.dp, top = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painterResource(R.drawable.ic_search),
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.width(4.dp))
                Text(
                    "Looked up ${Format.list(entry.lookedUp)}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/**
 * Models write Markdown; show its bold, italics, code and links, and leave
 * everything else as it is.
 */
private fun formatted(text: String): AnnotatedString = buildAnnotatedString {
    for (run in InlineMarkdown.parse(text)) {
        val style = SpanStyle(
            fontWeight = if (run.bold) FontWeight.Bold else null,
            fontStyle = if (run.italic) FontStyle.Italic else null,
            fontFamily = if (run.code) FontFamily.Monospace else null,
        )
        val link = run.link
        if (link != null) {
            val underlined = TextLinkStyles(style.copy(textDecoration = TextDecoration.Underline))
            withLink(LinkAnnotation.Url(link, underlined)) { append(run.text) }
        } else {
            withStyle(style) { append(run.text) }
        }
    }
}

/** Questions to start with. */
@Composable
private fun Suggestions(hasWind: Boolean, ask: (String) -> Unit) {
    Column(Modifier.padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            "Ask about your flying",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            if (hasWind) {
                "The assistant reads your logbook to answer, and the wind stations of your Venti server. " +
                    "It cannot see forecasts, airspace or other pilots yet."
            } else {
                "The assistant reads your logbook to answer. It cannot see weather, airspace or other pilots yet."
            },
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        for (question in if (hasWind) QUESTIONS + WIND_QUESTION else QUESTIONS) {
            OutlinedButton(onClick = { ask(question) }) {
                Text(question, textAlign = TextAlign.Start)
            }
        }
    }
}

private val QUESTIONS = listOf(
    "How was my flying this year?",
    "Which was my best flight, and why?",
    "Where did I find my strongest thermals?",
)

/** Offered as well when a Venti server is set. */
private const val WIND_QUESTION = "How is the wind on the Zugspitze right now?"

/**
 * Where the pilot enters the key for their own account, and reads what that
 * means.
 */
@Composable
private fun KeyEntry(chat: AssistantChat) {
    val links = LocalUriHandler.current
    var key by remember { mutableStateOf("") }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 24.dp)
            .widthIn(max = 560.dp),
    ) {
        Text(
            "The assistant answers questions about your flying from your logbook. It runs on your own " +
                "account with Anthropic, the maker of Claude: Venti pays for nothing and takes nothing.",
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(horizontal = 4.dp, vertical = 8.dp),
        )
        SectionHeader("Your key")
        OutlinedTextField(
            value = key,
            onValueChange = { key = it },
            label = { Text("API key") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(12.dp))
        Button(
            onClick = {
                chat.saveKey(key)
                key = ""
            },
            enabled = key.isNotBlank(),
        ) {
            Text("Save key")
        }
        SectionFooter(
            "Kept encrypted on this phone and sent only to Anthropic. Your questions, and what the " +
                "assistant looks up to answer them, go from this phone straight to Anthropic, which " +
                "bills your account for them. No Venti server sees your key or your questions. When " +
                "the assistant looks up live wind, the app asks your Venti server for the stations " +
                "around a place, as the map does.",
        )
        Spacer(Modifier.height(16.dp))
        TextButton(onClick = { links.openUri("https://console.anthropic.com/settings/keys") }) {
            Text("Get a key from Anthropic")
            Spacer(Modifier.width(6.dp))
            Icon(painterResource(R.drawable.ic_open_in_new), contentDescription = null, Modifier.size(18.dp))
        }
    }
}
