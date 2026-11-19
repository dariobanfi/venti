package fly.venti.app

import java.io.File
import java.nio.file.Files

/**
 * What the unit tests share. They run on the build machine, call the real
 * core through the generated interface, and read the shipped files from the
 * folder the build gathered them in.
 */
object TestSupport {
    /** The sample flight, the map style and the map region, as shipped in the app. */
    val files: AppFiles by lazy {
        FolderFiles(File(checkNotNull(System.getProperty("venti.assets")) { "run the tests through Gradle" }))
    }

    /** The folder those files are in, for tests that need real paths. */
    val assets: File get() = File(checkNotNull(System.getProperty("venti.assets")))

    val sampleFlight: String get() = files.text(AppFiles.SAMPLE_FLIGHT)

    /** A new, empty folder that is removed when the tests end. */
    fun newFolder(name: String): File =
        Files.createTempDirectory("venti-$name-").toFile().also { folder ->
            Runtime.getRuntime().addShutdownHook(Thread { folder.deleteRecursively() })
        }
}
