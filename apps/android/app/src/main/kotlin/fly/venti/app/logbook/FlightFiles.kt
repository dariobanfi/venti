package fly.venti.app.logbook

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream

/** Flight files as Android hands them over: from the file picker, or from another app. */
object FlightFiles {
    /**
     * The largest file read as a flight. A day's flight with a fix every
     * second is about a megabyte; the picker lets the pilot choose any file
     * at all, and a film must not be read into memory.
     */
    private const val LARGEST_BYTES = 32 * 1024 * 1024

    /**
     * A file the pilot picked or another app sent, ready to be imported.
     * `what` says what it was picked as, for the words about a problem.
     */
    fun source(context: Context, uri: Uri, what: String = "a flight file"): FlightFileSource =
        FlightFileSource(name(context, uri)) {
            val stream = try {
                context.contentResolver.openInputStream(uri)
            } catch (error: SecurityException) {
                throw IOException("Venti is not allowed to read this file.", error)
            } ?: throw IOException("The file could not be opened.")
            stream.use { input ->
                val bytes = input.readAtMost(LARGEST_BYTES + 1)
                if (bytes.size > LARGEST_BYTES) throw IOException("This file is too large to be $what.")
                LogbookModel.decode(bytes)
            }
        }

    /** The file's name as the pilot knows it, to report a problem with it. */
    private fun name(context: Context, uri: Uri): String {
        val named = runCatching {
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { row ->
                if (row.moveToFirst()) row.getString(0) else null
            }
        }.getOrNull()
        return named ?: uri.lastPathSegment ?: "File"
    }

    /** Reads to the end of the stream, or until `limit` bytes are there. */
    private fun InputStream.readAtMost(limit: Int): ByteArray {
        val bytes = ByteArrayOutputStream()
        val buffer = ByteArray(64 * 1024)
        while (bytes.size() < limit) {
            val read = read(buffer, 0, minOf(buffer.size, limit - bytes.size()))
            if (read < 0) break
            bytes.write(buffer, 0, read)
        }
        return bytes.toByteArray()
    }
}
