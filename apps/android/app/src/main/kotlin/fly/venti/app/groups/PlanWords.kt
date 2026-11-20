package fly.venti.app.groups

import fly.venti.core.GroupMember
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
            "then make a group or join one with the code its keeper gives you."

    const val NO_GROUP =
        "You are in no group yet. A group is a few pilots who tell each other where they mean to fly; " +
            "only its members see it."

    const val NO_PLANS = "Nobody in your groups has posted a plan for today or a later day."

    const val CUT_SHORT = "Your groups hold more plans than fit here: the later days are missing."

    const val WHO_SEES =
        "A plan is seen by the members of its group and nobody else, and is removed two days after its day."

    const val FORM_FOOT =
        "The members of the group see the plan, and nobody else. It is removed two days after its day."

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

/** What the groups' pages say, and what each thing a pilot can do to a group means. */
object GroupWords {
    const val WHAT_A_GROUP_IS =
        "A group is seen by its members and nobody else; it cannot be searched for. Whoever makes it keeps " +
            "it, and hands out its invite code."

    const val MAKING = "You will keep it: you hand out its invite code and can remove a member."

    const val JOINING = "The group's keeper has the code."

    const val THE_CODE =
        "Whoever has this code and an account on Venti can come into the group. It is shown this " +
            "once: Venti keeps no copy that can be read. A new code can be made at any time and ends " +
            "this one."

    const val KEEPING =
        "You keep this group. A member's menu removes them or hands the keeping on. Removing a member ends " +
            "the invite code, which they know."

    const val LEAVING_AS_KEEPER =
        "When you leave, the member who has been in longest keeps the group. Your plans go with you."

    const val LEAVING = "Your plans, and your word on the plans of others, go with you."

    /** Something that is asked about before it is done. */
    sealed interface Asking {
        data object NewCode : Asking

        data object Leave : Asking

        data object Delete : Asking

        data class Remove(val member: GroupMember) : Asking

        data class HandOver(val member: GroupMember) : Asking
    }

    fun question(asking: Asking): String = when (asking) {
        Asking.NewCode -> "Make a new invite code?"
        Asking.Leave -> "Leave this group?"
        Asking.Delete -> "Delete this group?"
        is Asking.Remove -> "Remove ${asking.member.name} from the group?"
        is Asking.HandOver -> "Make ${asking.member.name} the keeper?"
    }

    /** What it means, said under the question. */
    fun consequence(asking: Asking): String = when (asking) {
        Asking.NewCode -> "The code handed out before stops working. Members stay in the group."
        Asking.Leave -> "You will no longer see its plans, and yours are removed."
        Asking.Delete -> "The group goes for every member, with all its plans."
        is Asking.Remove ->
            "Their plans go with them, and the invite code, which they know, stops working: nobody comes " +
                "in until you make a new one."
        is Asking.HandOver ->
            "They will hand out the invite code and can remove members. You stay in the group as a member."
    }

    /** The button that does it. */
    fun confirm(asking: Asking): String = when (asking) {
        Asking.NewCode -> "Make a new code"
        Asking.Leave -> "Leave the group"
        Asking.Delete -> "Delete the group"
        is Asking.Remove -> "Remove"
        is Asking.HandOver -> "Make keeper"
    }

    /** Whether it takes something away for good, and is set in the warning colour. */
    fun isDestructive(asking: Asking): Boolean = asking is Asking.Leave || asking is Asking.Delete || asking is Asking.Remove

    /** A member's name in the list, with who it is. */
    fun name(member: GroupMember): String = if (member.isMe) "${member.name} (you)" else member.name

    /** A member's line for someone who cannot see it. */
    fun spoken(member: GroupMember): String =
        listOfNotNull("@${member.handle}", "Keeper".takeIf { member.isKeeper }).joinToString(", ")
}
