package fly.venti.app.logbook

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
     * What the pilot should be told, if anything. A clean import needs no
     * message: the new flights appear in the list.
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
            return null
        }
}

/** A message about an import, shown as a dialog. */
data class ImportNotice(val title: String, val text: String)
