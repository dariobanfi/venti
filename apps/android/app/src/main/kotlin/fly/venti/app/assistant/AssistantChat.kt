package fly.venti.app.assistant

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.core.Assistant
import fly.venti.core.AssistantRequest
import fly.venti.core.AssistantStep
import fly.venti.core.CoreException
import fly.venti.core.TranscriptEntry
import fly.venti.core.assistantModels
import fly.venti.core.assistantServiceAddress
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.IOException

/** Sends a request to the provider and returns its answer. */
typealias AssistantTransport = suspend (AssistantRequest) -> HttpAnswer

/** Where the assistant's model runs. */
enum class AssistantService(val stored: String) {
    /** Anthropic, with one of its models. */
    ANTHROPIC("anthropic"),

    /** Another service the pilot set by its address. */
    OTHER("other"),
}

/**
 * One conversation with the assistant, as the screen sees it.
 *
 * The conversation itself is the core's: what to send, which tools to run
 * against the logbook, what the reply is. This class does the one thing the
 * core cannot: it sends each request over the network and hands back the
 * answer. Requests go to the pilot's model provider; when the model looks
 * up live wind, the core names an address on the pilot's Venti server and
 * this class fetches it, without the key or the question.
 */
class AssistantChat(
    private val makeAssistant: (String) -> Assistant,
    private val keys: KeyStore,
    private val preferences: Preferences,
    /**
     * Where a question is worked on. It outlives the screen: an answer
     * that is on its way still arrives when the pilot has looked elsewhere.
     */
    private val scope: CoroutineScope,
    private val transport: AssistantTransport = AssistantHttp::send,
    /** Where the core works on a request, so the screen stays responsive. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
    /** How an address on the Venti server is fetched. */
    private val fetch: suspend (String) -> HttpAnswer = { url -> Http.get(url) },
    /** The pilot's Venti server, if one is set. With one, the assistant can look up live wind. */
    private val server: () -> String? = { null },
    /** The current time in seconds since 1970, by the server's clock where it is known. */
    private val now: () -> Long = { System.currentTimeMillis() / 1000 },
    /**
     * A conversation at another service than Anthropic, from its address
     * and its name for a model. Throws when the address is not one a key
     * may be sent to.
     */
    private val makeAssistantAt: (address: String, model: String) -> Assistant = { _, _ ->
        throw IllegalStateException("No other service can be used here.")
    },
    /** Where the key for another service is kept. */
    private val otherKeys: KeyStore = FixedKey(null),
) {
    var entries: List<TranscriptEntry> by mutableStateOf(emptyList())
        private set

    /** The question being answered right now, shown until the reply is in. */
    var pending: String? by mutableStateOf(null)
        private set

    var problem: String? by mutableStateOf(null)

    /** The model in use at Anthropic. */
    var model: String by mutableStateOf(preferences.assistantModel ?: assistantModels().firstOrNull()?.id.orEmpty())
        private set

    /** The other service's address and its name for the model, as the pilot last set them. Empty until then. */
    var otherAddress: String by mutableStateOf(preferences.assistantAddress.orEmpty())
        private set
    var otherModel: String by mutableStateOf(preferences.assistantOtherModel.orEmpty())
        private set

    private var assistant: Assistant

    /** Where the model runs. */
    var service: AssistantService by mutableStateOf(AssistantService.ANTHROPIC)
        private set

    init {
        // The other service is used only if it is still set up whole.
        val other = if (preferences.assistantService == AssistantService.OTHER.stored) otherAssistant() else null
        assistant = other ?: makeAssistant(model)
        if (other != null) service = AssistantService.OTHER
    }

    var hasKey: Boolean by mutableStateOf(key() != null)
        private set

    /** Whether a key for the other service is kept. */
    val hasOtherKey: Boolean get() = otherKeys.read() != null

    /** The key for the service in use. */
    private fun key(): String? = if (service == AssistantService.ANTHROPIC) keys.read() else otherKeys.read()

    /** A conversation at the other service as last set, or `null` if it is not set up whole. */
    private fun otherAssistant(): Assistant? =
        if (otherAddress.isEmpty()) null else runCatching { makeAssistantAt(otherAddress, otherModel) }.getOrNull()

    val isWorking: Boolean get() = pending != null

    /**
     * Asks a question. The answer arrives in `entries`; the returned job is
     * done when it has, or when it is clear that it will not.
     */
    fun ask(question: String): Job {
        val asked = question.trim()
        val key = key()
        if (asked.isEmpty() || pending != null || key == null) return Job().apply { complete() }
        pending = asked
        return scope.launch {
            try {
                answer(asked, key, assistant)
            } finally {
                pending = null
            }
        }
    }

    private suspend fun answer(question: String, key: String, assistant: Assistant) {
        assistant.useServer(server())
        var step = withContext(background) { assistant.ask(question, now(), key) }
        while (true) {
            val answer = when (val current = step) {
                is AssistantStep.Send -> try {
                    transport(current.request)
                } catch (error: IOException) {
                    assistant.abandon()
                    problem = "The provider could not be reached. ${error.pilotMessage}"
                    return
                } catch (error: CancellationException) {
                    assistant.abandon()
                    throw error
                }
                // A Venti server that cannot be reached does not end the
                // conversation: the model is told, and says so in its reply.
                is AssistantStep.Fetch -> try {
                    fetch(current.url)
                } catch (_: IOException) {
                    HttpAnswer(0, ByteArray(0))
                } catch (error: CancellationException) {
                    assistant.abandon()
                    throw error
                }
                is AssistantStep.Failed -> {
                    problem = current.message
                    break
                }
                AssistantStep.Finished -> break
            }
            val status = answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
            step = withContext(background) { assistant.receive(status, answer.body, key) }
        }
        // Unless the pilot started a new conversation meanwhile.
        if (assistant === this.assistant) {
            entries = assistant.transcript()
        }
    }

    /** Changes the model, which starts a new conversation. */
    fun use(model: String) {
        if (model == this.model) return
        this.model = model
        preferences.assistantModel = model
        startAgain()
    }

    fun saveKey(key: String) {
        val trimmed = key.trim()
        if (trimmed.isEmpty()) return
        if (keys.save(trimmed)) {
            hasKey = true
        } else {
            problem = "The key could not be stored on this phone."
        }
    }

    /** Removes the key of the service in use. Without its key another service is no longer used. */
    fun removeKey() {
        if (service == AssistantService.OTHER) {
            otherKeys.remove()
            useAnthropic()
        } else {
            keys.remove()
            hasKey = false
            startAgain()
        }
    }

    /**
     * Runs the assistant at another service than Anthropic: any that speaks
     * the chat completions format. The core says whether the address is one
     * a key may be sent to. A key typed now replaces the one kept for that
     * service; with none typed, the kept one stays. Returns whether it is
     * set up; if not, `problem` says why.
     */
    fun useOtherService(address: String, model: String, key: String): Boolean {
        val typed = key.trim()
        val named = model.trim()
        try {
            val made = assistantServiceAddress(address)
            val other = makeAssistantAt(made, named)
            if (typed.isNotEmpty() && !otherKeys.save(typed)) {
                problem = "The key could not be stored on this phone."
                return false
            }
            if (otherKeys.read() == null) {
                problem = "The service needs a key. A model you run yourself may take any text as one."
                return false
            }
            preferences.assistantService = AssistantService.OTHER.stored
            preferences.assistantAddress = made
            preferences.assistantOtherModel = named
            service = AssistantService.OTHER
            otherAddress = made
            otherModel = named
            assistant = other
            entries = emptyList()
            hasKey = true
            return true
        } catch (error: CoreException) {
            problem = error.pilotMessage
            return false
        }
    }

    /** Goes back to Anthropic. The other service's address and key stay kept, for changing back. */
    fun useAnthropic() {
        service = AssistantService.ANTHROPIC
        preferences.assistantService = AssistantService.ANTHROPIC.stored
        hasKey = keys.read() != null
        startAgain()
    }

    /** Forgets the conversation. */
    fun startAgain() {
        assistant = (if (service == AssistantService.OTHER) otherAssistant() else null) ?: makeAssistant(model)
        entries = emptyList()
    }

    companion object {
        /** The name of the computer an address points to: "api.example.org". */
        fun host(address: String): String = runCatching { java.net.URI(address).host }.getOrNull() ?: address
    }
}
