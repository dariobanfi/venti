package fly.venti.app

import android.content.Context
import java.io.File
import java.io.InputStream

/**
 * The files that ship inside the app: the sample flight, the map style, and
 * the folder with the label fonts and the demo map region.
 *
 * An interface, so the models can be tested on the build machine, where the
 * same files are read from the folder the build gathered them in.
 */
interface AppFiles {
    /** Opens a shipped file by its path, such as `sample.igc`. */
    fun open(path: String): InputStream

    /** The paths of every shipped file below a folder. */
    fun list(folder: String): List<String>

    fun text(path: String): String = open(path).use { it.readBytes().decodeToString() }

    companion object {
        const val SAMPLE_FLIGHT = "sample.igc"
        const val MAP_STYLE = "style.json"

        /** The notices of the crates compiled into the core (`licences/core-notices.txt` at the repository's root). */
        const val CORE_NOTICES = "licences/core-notices.txt"

        /** The label fonts, and a small map region for tests and offline development. */
        const val MAP_REGION = "MapDemo"
    }
}

/** The shipped files as the app finds them on a phone: its assets. */
class AssetFiles(private val context: Context) : AppFiles {
    override fun open(path: String): InputStream = context.assets.open(path)

    override fun list(folder: String): List<String> {
        val children = context.assets.list(folder).orEmpty()
        // The asset manager lists a file as a folder without children.
        return children.flatMap { child ->
            val path = "$folder/$child"
            list(path).ifEmpty { listOf(path) }
        }
    }
}

/** The shipped files in a folder on disk. The unit tests use it. */
class FolderFiles(private val folder: File) : AppFiles {
    override fun open(path: String): InputStream = folder.resolve(path).inputStream()

    override fun list(folder: String): List<String> =
        this.folder.resolve(folder).walkTopDown()
            .filter { it.isFile }
            .map { it.relativeTo(this.folder).invariantSeparatorsPath }
            .toList()
}
