package fly.venti.app

import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import java.io.File

/**
 * What the app hands to other apps: a flight's file, the pilot's waypoints,
 * the link to a flight. Files are written to one folder, the only one other
 * apps are ever given a file from.
 */
object SharedFiles {
    /** The folder a file is written to before it is shared. */
    fun folder(context: Context): File = context.cacheDir.resolve("venti-export")

    /** Offers a file to other apps: mail, a file manager, a league's uploader. */
    fun share(context: Context, file: File, title: String) {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
        val send = Intent(Intent.ACTION_SEND).apply {
            // Flight and waypoint files have no type of their own that Android knows.
            type = "application/octet-stream"
            putExtra(Intent.EXTRA_STREAM, uri)
            putExtra(Intent.EXTRA_TITLE, file.name)
            clipData = ClipData.newRawUri(file.name, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(Intent.createChooser(send, title))
    }

    /**
     * Hands a link to other apps as text: a messenger, a mail, a note. A
     * link to a flight is the key to it, so it goes nowhere but where the
     * pilot sends it.
     */
    fun shareText(context: Context, text: String, title: String) {
        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, text)
        }
        context.startActivity(Intent.createChooser(send, title))
    }
}
