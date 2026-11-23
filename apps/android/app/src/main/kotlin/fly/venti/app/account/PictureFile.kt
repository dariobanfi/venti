package fly.venti.app.account

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.net.Uri
import androidx.exifinterface.media.ExifInterface
import java.io.ByteArrayOutputStream
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * A photo the pilot chose as their picture, made ready to send: turned
 * upright as the camera noted, scaled so that its longer side is at most
 * [LONGEST_SIDE] pixels, and written anew as a JPEG. The server cuts the
 * middle square and scales it to 256 pixels; a photo of a phone's camera
 * as it is would be megabytes, and would carry where it was taken in its
 * metadata. Writing it anew sends none of that.
 */
object PictureFile {
    /** The longer side of what is sent, in pixels: "about 1,000", as the core asks. */
    const val LONGEST_SIDE = 1024

    /** The media type of what is sent. */
    const val MEDIA_TYPE = "image/jpeg"

    private const val QUALITY = 88

    /** Why a photo could not be read, in words for the pilot. */
    const val UNREADABLE = "This photo could not be read. Choose another one."

    /** The photo's bytes and media type, ready for the core's request; `null` where it cannot be read. */
    fun prepare(context: Context, photo: Uri): Pair<ByteArray, String>? {
        val resolver = context.contentResolver
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        resolver.openInputStream(photo)?.use { BitmapFactory.decodeStream(it, null, bounds) } ?: return null
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        val options = BitmapFactory.Options().apply { inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight, LONGEST_SIDE) }
        val decoded = resolver.openInputStream(photo)?.use { BitmapFactory.decodeStream(it, null, options) } ?: return null
        val turn = runCatching {
            resolver.openInputStream(photo)?.use { stream ->
                ExifInterface(stream).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)
            }
        }.getOrNull() ?: ExifInterface.ORIENTATION_NORMAL
        val (width, height) = scaled(decoded.width, decoded.height, LONGEST_SIDE)
        val matrix = Matrix().apply {
            postScale(width.toFloat() / decoded.width, height.toFloat() / decoded.height)
            when (turn) {
                ExifInterface.ORIENTATION_ROTATE_90 -> postRotate(90f)
                ExifInterface.ORIENTATION_ROTATE_180 -> postRotate(180f)
                ExifInterface.ORIENTATION_ROTATE_270 -> postRotate(270f)
                ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> postScale(-1f, 1f)
                ExifInterface.ORIENTATION_FLIP_VERTICAL -> postScale(1f, -1f)
            }
        }
        val upright = Bitmap.createBitmap(decoded, 0, 0, decoded.width, decoded.height, matrix, true)
        val bytes = ByteArrayOutputStream().use { out ->
            upright.compress(Bitmap.CompressFormat.JPEG, QUALITY, out)
            out.toByteArray()
        }
        if (upright !== decoded) upright.recycle()
        decoded.recycle()
        return bytes to MEDIA_TYPE
    }

    /**
     * The power of two to decode a photo at, so that what is decoded is
     * still at least `longest` pixels on its longer side: a 4000 by 3000
     * photo is read at a half, 2000 by 1500, since a quarter would be
     * smaller than asked.
     */
    fun sampleSize(width: Int, height: Int, longest: Int): Int {
        var sample = 1
        while (max(width, height) / (sample * 2) >= longest) sample *= 2
        return sample
    }

    /** A width and a height scaled down so that the longer side is at most `longest`; never up. */
    fun scaled(width: Int, height: Int, longest: Int): Pair<Int, Int> {
        val side = max(width, height)
        if (side <= longest) return width to height
        val factor = longest.toDouble() / side
        return max(1, (width * factor).roundToInt()) to max(1, (height * factor).roundToInt())
    }
}
