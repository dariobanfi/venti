package fly.venti.app.alerts

import fly.venti.core.AlertDayKind

/**
 * What the alarms' screens say that the core has not worded: the names of
 * the screens and the fields, and the line that asks a pilot signed out
 * to sign in. What an alarm waits for, what fired and what is wrong with
 * a draft are the core's.
 */
object AlertWords {
    const val ALERT_ME = "Alert me"
    const val TITLE = "Alerts"
    const val WHAT_FIRED = "What fired"
    const val NONE = "No alarms yet. Set one with “Alert me” on a wind station’s page or a take-off’s page."
    const val NOTHING_FIRED = "Nothing has fired in the last 30 days."
    const val SIGN_IN_FIRST = "Alarms live on your account: sign in to set one."
    const val SIGN_IN = "Sign in"
    const val NAME = "Name"
    const val FROM = "Wind from"
    const val TO = "To"
    const val ANY = "Any"
    const val LEAST_WIND = "Least wind"
    const val MOST_WIND = "Most wind"
    const val GUSTS = "Gusts at most"
    const val FROM_HOUR = "From"
    const val TO_HOUR = "Until"
    const val HOURS_NOTE = "Hours are Central European Time."
    const val DAYS = "Days"
    const val AT_LEAST = "At least"
    const val SAVE = "Save"
    const val DELETE = "Delete alarm"
    const val DELETE_QUESTION = "Delete this alarm?"
    const val CANCEL = "Cancel"
    const val OK = "OK"
    const val KMH = "km/h"
    const val CHANNEL = "Alarms"
    const val CHANNEL_ABOUT = "What your alarms on Venti fired."

    /** The days a day alarm can be for, 0 for today. */
    val DAY_NAMES = listOf("Today", "Tomorrow", "In 2 days", "In 3 days")

    /** The kinds of day, the least first, as the choice names them. */
    val DAY_KINDS = listOf(
        AlertDayKind.FLYABLE to "Flyable",
        AlertDayKind.LOCAL to "Local",
        AlertDayKind.CROSS_COUNTRY to "Cross-country",
        AlertDayKind.BIG to "Big",
    )

    /** An hour as the core's lines write it: "10:00". */
    fun hour(hour: Int): String = "%02d:00".format(java.util.Locale.ROOT, hour)

    /** What a screen reader says of an alarm's switch. */
    fun switch(name: String): String = "$name on"
}
