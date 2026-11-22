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

    /**
     * The choice of the kind of an alarm about the day the forecast makes,
     * beside the core's "Pilots in the air": the core has a word for the
     * second kind only.
     */
    const val DAY_KIND = "Flying day"

    const val SIGN_IN = "Sign in"
    const val CANCEL = "Cancel"
    const val OK = "OK"
    const val CHANNEL = "Alarms"
    const val CHANNEL_ABOUT = "What your alarms on Venti fired."

    /** An hour as the core's lines write it: "10:00". */
    fun hour(hour: Int): String = "%02d:00".format(java.util.Locale.ROOT, hour)

    /** What a screen reader says of an alarm's switch: its name, then the core's "On". */
    fun switch(name: String): String = "$name, ${core.on}"
}
