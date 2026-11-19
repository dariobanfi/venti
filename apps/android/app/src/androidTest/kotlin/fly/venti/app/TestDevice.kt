package fly.venti.app

import android.graphics.Bitmap
import android.os.ParcelFileDescriptor
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File

/** What the UI tests ask of the phone or emulator they run on, rather than of the app. */
object TestDevice {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    /** Runs a command as the device's shell and waits for it to finish. */
    fun shell(command: String): String =
        ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command))
            .use { it.readBytes().decodeToString() }

    /**
     * Keeps Android's own "isn't responding" dialogs off the screen. An
     * emulator that has just started is slow enough for its launcher to earn
     * one, and it would sit in the middle of every screenshot.
     */
    fun keepSystemDialogsAway() {
        shell("settings put global hide_error_dialogs 1")
        shell("am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS")
    }

    /**
     * Keeps a picture of the whole screen, the map and the system's bars
     * included, in the folder the build collects after the tests.
     */
    fun keepScreenshot(name: String) {
        val picture = checkNotNull(instrumentation.uiAutomation.takeScreenshot()) { "no screenshot for $name" }
        val folder = InstrumentationRegistry.getArguments().getString("additionalTestOutputDir")?.let(::File)
            ?: checkNotNull(instrumentation.targetContext.getExternalFilesDir("screenshots"))
        folder.mkdirs()
        File(folder, "$name.png").outputStream().use { picture.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
}
