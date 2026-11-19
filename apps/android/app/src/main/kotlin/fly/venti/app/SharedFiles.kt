package fly.venti.app

import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import java.io.File

/**
 * Files the app hands to other apps: a flight's file, the pilot's
 * waypoints. They are written to one folder, the only one other apps are
 * ever given a file from.
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
}
