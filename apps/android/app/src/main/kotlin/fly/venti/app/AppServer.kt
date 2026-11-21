package fly.venti.app

/**
 * The app's server: the one Venti, at the address the app was built with
 * (the Gradle property `venti.server`; a debug build without it talks to
 * the owner's own Venti on the build machine). The pilot sets none, and
 * nothing in the app knows where the address comes from but this. A UI
 * test names its own server instead, through `Session`'s launch options.
 */
object AppServer {
    val address: String = BuildConfig.VENTI_SERVER
}
