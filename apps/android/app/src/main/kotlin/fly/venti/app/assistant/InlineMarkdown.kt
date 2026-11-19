package fly.venti.app.assistant

/**
 * The little of Markdown a model's answer uses inside a sentence: bold,
 * italics, code and links. Line breaks and everything else stay as they
 * are, and a mark without its partner is shown as written.
 *
 * Android has no reader for Markdown, so this is the app's own, kept to
 * what a chat needs.
 */
object InlineMarkdown {
    /** A stretch of text set one way. */
    data class Run(
        val text: String,
        val bold: Boolean = false,
        val italic: Boolean = false,
        val code: Boolean = false,
        /** Where the stretch leads, if it is a link. */
        val link: String? = null,
    )

    fun parse(text: String): List<Run> = buildList { read(text, Run(""), this) }

    private fun read(text: String, style: Run, into: MutableList<Run>) {
        val plain = StringBuilder()

        fun flush() {
            if (plain.isNotEmpty()) {
                into += style.copy(text = plain.toString())
                plain.clear()
            }
        }

        var at = 0
        while (at < text.length) {
            val next = span(text, at, style)
            if (next == null) {
                plain.append(text[at])
                at += 1
                continue
            }
            flush()
            if (next.style.code) {
                // Code is shown as written, marks and all.
                into += next.style.copy(text = next.inner)
            } else {
                read(next.inner, next.style, into)
            }
            at = next.end
        }
        flush()
    }

    private class Span(val inner: String, val style: Run, val end: Int)

    /** The marked stretch that starts at `at`, if one does. */
    private fun span(text: String, at: Int, style: Run): Span? {
        val mark = text[at]
        return when {
            mark == '`' -> between(text, at, "`")?.let { Span(it, style.copy(code = true), at + it.length + 2) }
            text.startsWith("**", at) || text.startsWith("__", at) ->
                between(text, at, text.substring(at, at + 2))
                    ?.let { Span(it, style.copy(bold = true), at + it.length + 4) }
            mark == '*' || mark == '_' -> emphasis(text, at)?.let { Span(it, style.copy(italic = true), at + it.length + 2) }
            mark == '[' -> link(text, at, style)
            else -> null
        }
    }

    /** What stands between the mark at `at` and its next occurrence, if that is not empty. */
    private fun between(text: String, at: Int, mark: String): String? {
        val start = at + mark.length
        val end = text.indexOf(mark, start)
        return if (end > start) text.substring(start, end) else null
    }

    /**
     * Italics hug their words: "*so*", not "2 * 3". An underscore inside a
     * word, as in a file name, is not a mark at all.
     */
    private fun emphasis(text: String, at: Int): String? {
        val mark = text[at]
        val inner = between(text, at, mark.toString()) ?: return null
        val end = at + inner.length + 1
        if (inner.first().isWhitespace() || inner.last().isWhitespace() || '\n' in inner) return null
        if (mark == '_') {
            val before = text.getOrNull(at - 1)
            val after = text.getOrNull(end + 1)
            if (before?.isLetterOrDigit() == true || after?.isLetterOrDigit() == true) return null
        }
        return inner
    }

    /** `[words](https://...)`. Only web addresses become links. */
    private fun link(text: String, at: Int, style: Run): Span? {
        val middle = text.indexOf("](", at + 1)
        if (middle <= at + 1) return null
        val end = text.indexOf(')', middle + 2)
        if (end < 0) return null
        val address = text.substring(middle + 2, end)
        if (!address.startsWith("https://") && !address.startsWith("http://")) return null
        if ('\n' in text.substring(at, end)) return null
        return Span(text.substring(at + 1, middle), style.copy(link = address), end + 1)
    }
}
