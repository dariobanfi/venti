package fly.venti.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Venti among the phone's apps, with its icon, as the launcher shows it: a
 * picture for the owner, who cannot otherwise see the icon an emulator
 * shows. The launcher is the system's own, so it is driven from the shell
 * and read from its own view tree: the picture is taken once the launcher
 * says it shows Venti.
 */
@RunWith(AndroidJUnit4::class)
class LauncherIconTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    /** What the launcher shows, as the system's own tool writes it out. */
    private fun shown(): String {
        TestDevice.shell("uiautomator dump $DUMP")
        return TestDevice.shell("cat $DUMP")
    }

    /** Whether Venti's icon, with its name under it, is among what the launcher shows. */
    private fun showsVenti(): Boolean = shown().let { "text=\"Venti\"" in it || "content-desc=\"Venti\"" in it }

    /** Asks the launcher again until it shows Venti, as many times at most. */
    private fun waitForVenti(looks: Int): Boolean = (1..looks).any { showsVenti() }

    @Test
    fun theAppHasItsIconAmongThePhonesApps() {
        TestDevice.keepSystemDialogsAway()
        val app = instrumentation.targetContext.packageName
        // The launcher's home screen, then its list of every app, which is
        // where a launcher puts an app nobody has placed on a home screen.
        TestDevice.shell("input keyevent KEYCODE_HOME")
        val size = Regex("""(\d+)x(\d+)""").find(TestDevice.shell("wm size"))
        val width = size?.groupValues?.get(1)?.toInt() ?: 1080
        val height = size?.groupValues?.get(2)?.toInt() ?: 2400
        TestDevice.shell("input swipe ${width / 2} ${height * 9 / 10} ${width / 2} ${height / 4} 300")
        // The list's search finds it by its name, wherever it stands.
        TestDevice.shell("input text Venti")
        // The launcher is asked what it shows until it shows Venti; the
        // system's tool cannot always read the launcher while a test runs,
        // so this waits and does not judge. The picture is what counts.
        waitForVenti(looks = 10)
        TestDevice.keepScreenshot("icon-1-launcher")
        // The app is one the launcher lists: it has an activity for it.
        val launcher = TestDevice.shell("cmd package resolve-activity --brief -c android.intent.category.LAUNCHER $app")
        assertTrue(launcher, launcher.contains("$app/"))
        TestDevice.shell("input keyevent KEYCODE_HOME")
    }

    private companion object {
        /** Where the system's tool writes what the launcher shows. */
        const val DUMP = "/data/local/tmp/venti-launcher.xml"
    }
}
