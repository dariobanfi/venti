package fly.venti.app.assistant

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.pilotMessage
import fly.venti.app.settings.Preferences
import fly.venti.core.Assistant
import fly.venti.core.AssistantRequest
import fly.venti.core.AssistantStep
import fly.venti.core.TranscriptEntry
import fly.venti.core.assistantModels
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.IOException

/** What a provider answered: the HTTP status and the body. */
class ProviderAnswer(val status: Int, val body: ByteArray)

/** Sends a request to the provider and returns its answer. */
typealias AssistantTransport = suspend (AssistantRequest) -> ProviderAnswer

/**
 * One conversation with the assistant, as the screen sees it.
 *
 * The conversation itself is the core's: what to send, which tools to run
 * against the logbook, what the reply is. This class does the one thing the
 * core cannot: it sends each request over the network and hands back the
 * answer.
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
) {
    var entries: List<TranscriptEntry> by mutableStateOf(emptyList())
        private set

    /** The question being answered right now, shown until the reply is in. */
    var pending: String? by mutableStateOf(null)
        private set

    var problem: String? by mutableStateOf(null)

    var hasKey: Boolean by mutableStateOf(keys.read() != null)
        private set

    /** The model in use. */
    var model: String by mutableStateOf(preferences.assistantModel ?: assistantModels().firstOrNull()?.id.orEmpty())
        private set

    private var assistant = makeAssistant(model)

    val isWorking: Boolean get() = pending != null

    /**
     * Asks a question. The answer arrives in `entries`; the returned job is
     * done when it has, or when it is clear that it will not.
     */
    fun ask(question: String): Job {
        val asked = question.trim()
        val key = keys.read()
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
        var step = withContext(background) { assistant.ask(question, System.currentTimeMillis() / 1000, key) }
        while (step is AssistantStep.Send) {
            val answer = try {
                transport(step.request)
            } catch (error: IOException) {
                assistant.abandon()
                problem = "The provider could not be reached. ${error.pilotMessage}"
                return
            } catch (error: CancellationException) {
                assistant.abandon()
                throw error
            }
            val status = answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
            step = withContext(background) { assistant.receive(status, answer.body, key) }
        }
        if (step is AssistantStep.Failed) {
            problem = step.message
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

    fun removeKey() {
        keys.remove()
        hasKey = false
        startAgain()
    }

    /** Forgets the conversation. */
    fun startAgain() {
        assistant = makeAssistant(model)
        entries = emptyList()
    }
}
