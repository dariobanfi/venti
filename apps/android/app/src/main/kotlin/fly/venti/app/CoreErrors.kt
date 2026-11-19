package fly.venti.app

import fly.venti.core.CoreException

/**
 * What went wrong, in words fit to show to the pilot.
 *
 * Every error of the core carries such a text. The generated interface keeps
 * it in a field called `reason` (see `uniffi.toml`), because a Kotlin
 * exception's own `message` is taken by a description of the fields.
 */
val Throwable.pilotMessage: String
    get() = when (this) {
        is CoreException.InvalidFlight -> reason
        is CoreException.UnknownFlight -> reason
        is CoreException.Storage -> reason
        else -> localizedMessage ?: toString()
    }
