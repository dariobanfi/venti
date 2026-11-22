package fly.venti.app.task

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.EncodeHintType
import com.google.zxing.LuminanceSource
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.WriterException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import com.google.zxing.qrcode.encoder.Encoder

/**
 * A text as a QR code and back: the dots to draw for a task, and the text
 * of a code in a picture.
 *
 * What the text of a task's code is, and whether a text read from a code
 * is a task, is the core's (`keptTaskCode`, `keepTask`). This is only the
 * code itself, which ZXing writes and reads.
 */
object TaskCode {
    /** The dots of a code: a square of `side` by `side`, each dark or light. */
    class Dots(val side: Int, private val dark: BooleanArray) {
        /** Whether the dot in this column and row, counted from the upper left, is dark. */
        fun isDark(column: Int, row: Int): Boolean = dark[row * side + column]
    }

    /**
     * The empty room a code needs around it to be found, in dots: what the
     * standard asks for.
     */
    const val QUIET_DOTS = 4

    /**
     * The dots of the code for a text, or `null` where a code cannot hold
     * it. With the middle level of error correction, as on the iPhone: a
     * code that still reads with a reflection on it, and not more dots
     * than that takes.
     */
    fun dots(text: String): Dots? {
        val matrix = try {
            Encoder.encode(text, ErrorCorrectionLevel.M, mapOf(EncodeHintType.CHARACTER_SET to "UTF-8")).matrix
        } catch (_: WriterException) {
            return null
        }
        val side = matrix.width
        return Dots(side, BooleanArray(side * side) { matrix.get(it % side, it / side).toInt() == 1 })
    }

    /**
     * The text of the QR code in a picture given as its pixels, row by
     * row, or `null` where none is found.
     */
    fun read(pixels: IntArray, width: Int, height: Int): String? = read(RGBLuminanceSource(width, height, pixels))

    /**
     * The text of the QR code in a camera's frame, given as its plane of
     * brightness with `rowWidth` bytes to a row, or `null` where none is
     * found.
     */
    fun read(brightness: ByteArray, rowWidth: Int, width: Int, height: Int): String? =
        read(PlanarYUVLuminanceSource(brightness, rowWidth, height, 0, 0, width, height, false))

    private fun read(picture: LuminanceSource): String? = try {
        QRCodeReader().decode(BinaryBitmap(HybridBinarizer(picture)), HINTS).text
    } catch (_: ReaderException) {
        // No code in view, or one that cannot be made out yet.
        null
    }

    private val HINTS = mapOf(
        DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
        DecodeHintType.CHARACTER_SET to "UTF-8",
    )
}
