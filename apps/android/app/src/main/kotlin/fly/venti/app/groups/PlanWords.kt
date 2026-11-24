package fly.venti.app.groups

import fly.venti.app.design.Format
import fly.venti.core.PlanView
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.util.Locale

/** How a plan's day and time are written and said, and what the board says where it has nothing. */
object PlanWords {
    /** The row that takes a plan's place from the pilot's waypoints. */
    const val FROM_WAYPOINTS = "From my waypoints"

    const val SIGNED_OUT =
        "Plans for a flying day are shared in groups on Venti. Sign in under Settings, Account, " +
            "then make a group or join one."

    const val NO_GROUP =
        "You are in no group yet. A group is a few pilots who tell each other where they mean to fly; " +
            "only its members see it."

    const val NO_PLANS = "Nobody in your groups has posted a plan for today or a later day."

    const val CUT_SHORT = "Your groups hold more plans than fit here: the later days are missing."

    const val WHO_SEES =
        "A plan is seen by the members of its group and nobody else, and is removed two days after its day."

    const val FORM_FOOT =
        "The plan is posted in the group's chat, with \"I'm in\" on its card. It is removed two days after its day."

    const val GONE = "This plan is no longer there."

    /** A day as the server writes it, "2026-11-21", in the pilot's own calendar. */
    fun day(date: LocalDate): String = DateTimeFormatter.ISO_LOCAL_DATE.format(date)

    /** The date of a day written "2026-11-21"; `null` for anything else. */
    fun date(day: String): LocalDate? = runCatching { LocalDate.parse(day, DateTimeFormatter.ISO_LOCAL_DATE) }.getOrNull()

    /** A time of day as the server writes it, "09:30". */
    fun timeOfDay(hour: Int, minute: Int): String = String.format(Locale.ROOT, "%02d:%02d", hour, minute)

    /** The hour and the minute of a time written "09:30"; `null` for anything else. */
    fun time(meetAt: String): Pair<Int, Int>? {
        val parts = meetAt.split(":").map { it.toIntOrNull() }
        val hour = parts.getOrNull(0)
        val minute = parts.getOrNull(1)
        return if (parts.size == 2 && hour != null && minute != null && hour in 0..23 && minute in 0..59) {
            hour to minute
        } else {
            null
        }
    }

    /** "Today", "Tomorrow", or the day with its weekday: "Saturday, 21 Nov". */
    fun title(day: String, today: LocalDate = LocalDate.now()): String {
        val date = date(day) ?: return day
        return when (date) {
            today -> "Today"
            today.plusDays(1) -> "Tomorrow"
            else -> DateTimeFormatter.ofPattern("EEEE, d MMM").format(date)
        }
    }

    /** One plan's line for someone who cannot see it: the place is its name, this the rest. */
    fun spoken(plan: PlanView): String = listOfNotNull(
        plan.meetAt?.let { "At $it." },
        plan.inWords.takeIf { it.isNotEmpty() }?.let { "$it." },
        plan.groupName.takeIf { it.isNotEmpty() }?.let { "$it." },
        "You are in.".takeIf { plan.amIn },
    ).joinToString(" ")
}

/**
 * What the groups' screens say that neither an answer nor the core's
 * `communityWords()` carries: the iPhone's words (`GroupsScreens.swift`,
 * `ChatScreen.swift`), word for word.
 */
object GroupWords {
    const val SIGNED_OUT = "Groups are shared on Venti with an account. Sign in under Settings, Account."
    const val INVITES = "Invites"
    const val DECLINE = "Decline"
    const val FOUND = "Found"
    const val NONE_YET = "You are in no group yet. Join one near you, or make one."
    const val NONE_YET_LONG =
        "You are in no group yet. A group is a chat of pilots who fly together: public ones near you to join, or a private one of your own."
    const val FIND_OR_MAKE = "Find or make one"
    const val ALL = "All"
    const val NEW_GROUP = "New group"
    const val CHANGE_GROUP = "Change the group"
    const val ABOUT = "About the group"
    const val NEW_PLAN = "New plan"
    const val MEMBERS = "Members"
    const val HOME = "Home"
    const val INVITE_A_PILOT = "Invite a pilot"
    const val INVITE_BY_HANDLE = "Invite by handle"
    const val BY_THE_HANDLE = "By the handle on their pilot's page."
    const val HANDLE = "@handle"
    const val EDIT = "Change name, description and home"
    const val GIVE_PICTURE = "Give it a picture"
    const val CHANGE_PICTURE = "Change the picture"
    const val ANOTHER_PICTURE = "Another picture"
    const val REMOVE_PICTURE = "Remove the picture"
    const val PICTURE_NOTE = "Optional. Cut square."
    const val PUBLIC_NOTE = "Anyone signed in finds, reads and joins a public group."
    const val PRIVATE_NOTE = "Only members read a private group; you and its admins invite pilots into it."
    const val FORM_PUBLIC_NOTE = "Anyone signed in finds, reads and joins it."
    const val FORM_PRIVATE_NOTE = "Only its members read it; you and its admins invite pilots by handle."
    const val MUTE_PUBLIC = "A public group's messages reach your phone only where they mention you."
    const val MUTE_PRIVATE = "Each new message reaches your phone unless the group is muted."
    const val DELETE = "Delete the group"
    const val HAND_ON_FIRST = "Tap a member to hand the group on to them before you leave."
    const val REPORT_GROUP = "Report this group"
    const val LEAVE_QUESTION = "Leave this group?"
    const val DELETE_QUESTION = "Delete this group?"
    const val LEAVE_MEANS = "Its messages stay; your plans in it go with you."
    const val DELETE_MEANS = "The group goes for every member, with its messages and plans."
    const val MAKE_ADMIN = "Make admin"
    const val MAKE_MEMBER = "Make a member again"
    const val REMOVE_MEMBER = "Remove from the group"
    const val NAME = "Name"
    const val ABOUT_FIELD = "What the group is about"
    const val HOME_AS_IT_IS = "As it is"
    const val HOME_MAP = "Where the map is"
    const val HOME_POSITION = "My position"
    const val HOME_NONE = "None"
    const val PLACE_FIELD = "The place, such as Inn valley"
    const val HOME_NOTE = "Pilots near it find the group."
    const val NO_POSITION = "Venti may not read the position."
    const val WAITING_FOR_POSITION = "Waiting for the position…"
    const val POSITION_UNKNOWN = "Venti does not know the position yet."
    const val SAVE = "Save"
    const val REMOVE = "Remove"
    const val REMOVE_QUESTION = "Remove this message?"
    const val REMOVE_MEANS = "It keeps its place in the chat, without its words, pictures and links."
    const val REPORT_MESSAGE = "Report this message"
    const val REPORT_THE_PICTURE = "Report the picture"
    const val REPORT_THIS_PICTURE = "Report this picture"
    const val PICTURE = "Picture"
    const val ADD_PHOTOS = "Add photos"
    const val TAKE_PHOTO_OUT = "Take the photo out"

    /** The radius a home can have, kilometres, and its step. */
    const val LEAST_RADIUS = 5.0
    const val MOST_RADIUS = 100.0
    const val RADIUS_STEP = 5.0

    /** At most this many photos go with a message (`docs/api.md`). */
    const val MOST_PICTURES = 4

    fun owner(name: String): String = "Make $name the owner"

    fun you(name: String): String = "$name (you)"

    fun unread(badge: String): String = "$badge unread"

    fun reportPicture(index: Int, count: Int): String = if (count == 1) REPORT_THE_PICTURE else "Report picture ${index + 1}"

    fun around(radiusKm: Double): String = "${Format.whole(radiusKm)} km around"

    fun home(radiusKm: Double, place: String): String = "${Format.whole(radiusKm)} km around $place"

    /** How many links of a message's text become cards, in a line; `null` for none. */
    fun links(count: Int): String? = when (count) {
        0 -> null
        1 -> "The link shows as a card."
        else -> "$count links show as cards."
    }

    /** The invites into groups in a line, on Community. */
    fun invites(words: List<String>): String? = when (words.size) {
        0 -> null
        1 -> words.single()
        else -> "${words.size} invites into groups"
    }
}
