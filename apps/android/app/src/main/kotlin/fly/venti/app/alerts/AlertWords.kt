package fly.venti.app.alerts

import fly.venti.core.AlertFormWords
import fly.venti.core.alertFormWords

/**
 * The words of the alarms' screens. Every label, title, day and kind of
 * day, the empty and signed-out lines and the fields' names are the
 * core's (`alertFormWords`), the same in the three apps; what an alarm
 * waits for, what fired and what is wrong with a draft are the core's too.
 * Here stand only the words of Android's own pieces: the buttons of a
 * dialog, the notification channel, what a screen reader says of a switch.
 */
object AlertWords {
    /** The core's words, read once. */
    val core: AlertFormWords by lazy { alertFormWords() }

    const val CHANNEL = "Alarms"
    const val CHANNEL_ABOUT = "What your alarms on Venti fired."

    /** An hour as the core's lines write it: "10:00". */
    fun hour(hour: Int): String = "%02d:00".format(java.util.Locale.ROOT, hour)

    /** What a screen reader says of an alarm's switch: its name, then the core's "On". */
    fun switch(name: String): String = "$name, ${core.on}"
}
