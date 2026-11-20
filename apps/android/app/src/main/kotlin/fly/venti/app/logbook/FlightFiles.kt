package fly.venti.app.logbook

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream

/** A file the pilot picked: its name, and a way to read it. */
class PickedFile(val name: String, val read: () -> ByteArray)

/** Flight files as Android hands them over: from the file picker, or from another app. */
object FlightFiles {
    /**
     * The largest file read as a flight. A day's flight with a fix every
     * second is about a megabyte; the picker lets the pilot choose any file
     * at all, and a film must not be read into memory.
     */
    private const val LARGEST_BYTES = 32 * 1024 * 1024

    /** How many folders deep a picked folder is searched. No pilot files flights deeper; a loop of links would be. */
    private const val DEEPEST_FOLDER = 8

    /** What is asked about each entry of a folder: how to reach it, its name, and whether it is a folder. */
    private val FOLDER_COLUMNS = arrayOf(
        DocumentsContract.Document.COLUMN_DOCUMENT_ID,
        DocumentsContract.Document.COLUMN_DISPLAY_NAME,
        DocumentsContract.Document.COLUMN_MIME_TYPE,
        DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        DocumentsContract.Document.COLUMN_SIZE,
    )

    /**
     * A file the pilot picked or another app sent, ready to be imported as
     * text. `what` says what it was picked as, for the words about a
     * problem.
     */
    fun source(context: Context, uri: Uri, what: String = "a flight file"): FlightFileSource {
        val file = picked(context, uri, what)
        return FlightFileSource(file.name) { LogbookModel.decode(file.read()) }
    }

    /** The same file as it is, for a reader that looks at the bytes itself. */
    fun picked(context: Context, uri: Uri, what: String): PickedFile =
        PickedFile(name(context, uri)) { read(context, uri, what) }

    private fun read(context: Context, uri: Uri, what: String): ByteArray {
        val stream = try {
            context.contentResolver.openInputStream(uri)
        } catch (error: SecurityException) {
            throw IOException("Venti is not allowed to read this file.", error)
        } ?: throw IOException("The file could not be opened.")
        return stream.use { input ->
            val bytes = input.readAtMost(LARGEST_BYTES + 1)
            if (bytes.size > LARGEST_BYTES) throw IOException("This file is too large to be $what.")
            bytes
        }
    }

    /**
     * Every flight file in a folder the pilot picked and in the folders
     * inside it, ready to be imported. Asking Android for a folder's
     * contents takes a while for a large one: call this away from the
     * screen.
     */
    fun inFolder(context: Context, tree: Uri): List<FlightFileSource> = keyedInFolder(context, tree).map { it.second }

    /**
     * The same, each with what tells a file apart from what it was: where
     * it lies in the folder, its size and when it was last changed. A
     * watched folder brings in only the files whose key it has not seen.
     */
    fun keyedInFolder(context: Context, tree: Uri): List<Pair<String, FlightFileSource>> {
        val found = mutableListOf<Pair<String, Pair<Uri, String>>>()

        fun list(folder: String, path: String, depth: Int) {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, folder)
            val rows = try {
                context.contentResolver.query(children, FOLDER_COLUMNS, null, null, null)
            } catch (error: SecurityException) {
                throw IOException("Venti is not allowed to read this folder.", error)
            } ?: throw IOException("The folder could not be opened.")
            rows.use { row ->
                while (row.moveToNext()) {
                    val id = row.getString(0)
                    val name = row.getString(1) ?: continue
                    // Hidden files and folders are whatever an app left behind, not the pilot's.
                    if (name.startsWith(".")) continue
                    if (row.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR) {
                        if (depth < DEEPEST_FOLDER) list(id, "$path$name/", depth + 1)
                    } else {
                        val key = "$path$name|${row.getLong(4)}|${row.getLong(3)}"
                        found += "$path$name" to (DocumentsContract.buildDocumentUriUsingTree(tree, id) to key)
                    }
                }
            }
        }
        list(DocumentsContract.getTreeDocumentId(tree), "", depth = 0)

        return flightFilesAmong(found).map { (path, file) ->
            val (uri, key) = file
            key to FlightFileSource(path.substringAfterLast('/')) { LogbookModel.decode(read(context, uri, "a flight file")) }
        }
    }

    /**
     * Of the files found in a folder, each with where it lies inside it,
     * those to import. A file picked by hand is tried whatever its name; in
     * a folder only names ending in `.igc` count, so that a folder of
     * photos is not a list of failures. Recorders name files by date:
     * oldest first reads naturally in the list as it fills.
     */
    fun <T> flightFilesAmong(found: List<Pair<String, T>>): List<Pair<String, T>> =
        found.filter { (path, _) -> path.endsWith(".igc", ignoreCase = true) }.sortedBy { (path, _) -> path }

    /** What to call a folder the pilot picked, to report a problem with it. */
    fun folderName(context: Context, tree: Uri): String = runCatching {
        name(context, DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree)))
    }.getOrDefault("The folder")

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
