package fly.venti.app.logbook

import fly.venti.app.design.Format

/** What happened to a batch of imported files. */
data class ImportReport(
    /** Identifiers of the flights that were added. */
    val added: List<String> = emptyList(),
    /** Identifiers of the flights the logbook already held. */
    val alreadyThere: List<String> = emptyList(),
    val failures: List<Failure> = emptyList(),
) {
    data class Failure(val file: String, val reason: String)

    /**
     * What the pilot should be told, if anything. One new flight needs no
     * message: it opens. A batch says in a sentence what it did.
     */
    val notice: ImportNotice?
        get() {
            if (failures.isNotEmpty()) {
                val shown = failures.take(4).map { "${it.file}: ${it.reason}" }
                val more = if (failures.size > 4) listOf("and ${failures.size - 4} more.") else emptyList()
                return ImportNotice(
                    title = if (failures.size == 1) {
                        "1 file could not be imported"
                    } else {
                        "${failures.size} files could not be imported"
                    },
                    text = (shown + more).joinToString("\n"),
                )
            }
            if (added.isEmpty() && alreadyThere.isNotEmpty()) {
                return ImportNotice(
                    title = "Already in your logbook",
                    text = if (alreadyThere.size == 1) {
                        "This flight was imported before. Nothing was added."
                    } else {
                        "These flights were imported before. Nothing was added."
                    },
                )
            }
            if (added.size > 1 || (added.isNotEmpty() && alreadyThere.isNotEmpty())) {
                val before = when (alreadyThere.size) {
                    0 -> ""
                    1 -> " 1 was in your logbook already."
                    else -> " ${Format.count(alreadyThere.size.toLong())} were in your logbook already."
                }
                return ImportNotice(
                    title = if (added.size == 1) "1 flight added" else "${Format.count(added.size.toLong())} flights added",
                    text = (if (added.size == 1) "It is" else "They are") + " in your logbook, on this phone.$before",
                )
            }
            if (added.isEmpty() && alreadyThere.isEmpty()) {
                return ImportNotice("No flights found", "There was no flight file among what you chose.")
            }
            return null
        }
}

/** A message about an import, shown as a dialog. */
data class ImportNotice(val title: String, val text: String)
