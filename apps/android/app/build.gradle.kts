import org.gradle.process.ExecOperations
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

/** The Android ABIs the Rust core is built for; see gradle.properties. */
val coreAbis: Provider<List<String>> = providers.gradleProperty("venti.abis").map { it.split(',') }

android {
    namespace = "fly.venti.app"
    compileSdk = 37
    // The NDK links the Rust core. This is the one place its version is set:
    // the cargo task below is handed the same folder.
    ndkVersion = "29.0.14206865"

    defaultConfig {
        // The same identifier as the iPhone app; to be confirmed once the
        // venti.fly domain is registered.
        applicationId = "fly.venti.app"
        // Android 8: notification channels, foreground services and the
        // Bluetooth LE calls the cockpit uses all exist from here on.
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // Only the ABIs the core is built for go into the app. Without the
        // core for a phone's processor the app would install and not start.
        ndk {
            abiFilters += coreAbis.get()
        }
    }

    buildTypes {
        release {
            // No shrinking yet: the generated bridge reaches the core by
            // reflection, and nobody has checked a shrunk build on a phone.
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    androidResources {
        // Map tiles are compressed inside their archives already.
        noCompress += listOf("pmtiles", "pbf")
    }

    testOptions {
        animationsDisabled = true
    }

    lint {
        // The bridge is generated; its style is not ours to fix.
        checkGeneratedSources = false
        warningsAsErrors = true
        // Newer library versions are a decision, not a defect. And lint
        // cannot read the ABI list from gradle.properties, where x86_64 is.
        disable += listOf(
            "GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion", "ChromeOsAbiSupport",
        )
    }
}

dependencies {
    implementation(libs.androidx.core)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.maplibre)
    // The generated bridge calls the core through JNA. On a phone that is
    // the Android archive, with JNA's own native library for each ABI.
    implementation(libs.jna) { artifact { type = "aar" } }

    testImplementation(libs.junit)
    // On the build machine it is the plain jar, with the desktop libraries.
    testImplementation(libs.jna)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
}

// ---------------------------------------------------------------------------
// The Rust core and the files shared with the other apps.
//
// Nothing below is committed as a copy. The core is compiled by cargo, its
// Kotlin interface is generated from the compiled library, and the sample
// flight, the map style and the bundled map region are taken from where the
// repository keeps them.
// ---------------------------------------------------------------------------

val repositoryRoot: Directory = rootProject.layout.projectDirectory.dir("../..")

/** What the core is built from. A change to any of it rebuilds the core. */
val coreSources: FileCollection = files(
    repositoryRoot.file("Cargo.toml"),
    repositoryRoot.file("Cargo.lock"),
    repositoryRoot.file("rust-toolchain.toml"),
    // The core and every crate of the workspace it is built from.
    fileTree(repositoryRoot.dir("crates")) {
        include(
            "core/**", "flight/**", "sensors/**", "domain/**",
            "airspace/**", "api-types/**", "llm/**", "tools/**",
        )
    },
    fileTree(repositoryRoot.dir("tools/uniffi-bindgen-swift")),
)

/** Builds `libventi_core.so` for each Android ABI with cargo-ndk. */
abstract class CoreForAndroid : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val minSdk: Property<Int>
    @get:Internal abstract val repository: DirectoryProperty
    @get:Internal abstract val ndk: DirectoryProperty
    @get:OutputDirectory abstract val jniLibs: DirectoryProperty

    @TaskAction
    fun build() {
        val command = mutableListOf("cargo", "ndk", "--platform", minSdk.get().toString())
        abis.get().forEach { command += listOf("--target", it) }
        command += listOf("--output-dir", jniLibs.get().asFile.path)
        command += listOf("build", "--release", "--locked", "-p", "venti-core")
        exec.exec {
            workingDir = repository.get().asFile
            environment("ANDROID_NDK_HOME", ndk.get().asFile.path)
            commandLine(command)
        }
    }
}

/**
 * Builds the core for the machine the build runs on and generates the Kotlin
 * interface from that library. The library itself is kept for the unit
 * tests, which call the real core on the JVM.
 */
abstract class CoreBindings : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations
    @get:Inject abstract val files: FileSystemOperations

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val config: RegularFileProperty

    @get:Internal abstract val repository: DirectoryProperty
    @get:OutputDirectory abstract val kotlin: DirectoryProperty
    @get:OutputDirectory abstract val hostLibrary: DirectoryProperty

    @TaskAction
    fun generate() {
        val root = repository.get().asFile
        val target = root.resolve("target")
        exec.exec {
            workingDir = root
            commandLine(
                "cargo", "build", "--release", "--locked", "-p", "venti-core",
                "--target-dir", target.path,
            )
        }
        // libventi_core.so on Linux, libventi_core.dylib on a Mac.
        val library = target.resolve("release").resolve(System.mapLibraryName("venti_core"))
        files.sync {
            from(library)
            into(hostLibrary)
        }
        files.delete { delete(kotlin) }
        exec.exec {
            workingDir = root
            commandLine(
                "cargo", "run", "--locked", "-p", "uniffi-bindgen-swift", "--bin", "uniffi-bindgen",
                "--target-dir", target.path, "--",
                "generate", "--language", "kotlin", "--no-format",
                "--config", config.get().asFile.path,
                "--out-dir", kotlin.get().asFile.path,
                library.path,
            )
        }
    }
}

/** Gathers the files the app shares with the other apps into one assets folder. */
abstract class SharedAssets : DefaultTask() {
    @get:Inject abstract val files: FileSystemOperations

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val sampleFlight: RegularFileProperty

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val mapStyle: RegularFileProperty

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val mapRegion: DirectoryProperty

    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val fontLicence: RegularFileProperty

    @get:OutputDirectory abstract val assets: DirectoryProperty

    @TaskAction
    fun gather() {
        files.sync {
            from(sampleFlight)
            from(mapStyle)
            // The fonts and the demo region keep their folder: the map
            // engine asks for fonts by folder and file name.
            from(mapRegion) { into("MapDemo") }
            // The licence of the design pack's fonts travels with them.
            from(fontLicence) { into("fonts") }
            into(assets)
        }
    }
}

/**
 * Takes the owner's design pack, `design/` at the repository's root, into
 * the build: its Kotlin tokens as they are, the rest of its tokens as Kotlin
 * made from `tokens.json`, its fonts, its flying icons as vector drawables,
 * and the launcher and notification icons. Nothing of it is copied into the
 * app's own folder.
 */
abstract class DesignPack : DefaultTask() {
    @get:Inject abstract val files: FileSystemOperations

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val pack: DirectoryProperty

    @get:OutputDirectory abstract val kotlin: DirectoryProperty
    @get:OutputDirectory abstract val res: DirectoryProperty

    @TaskAction
    fun gather() {
        val design = pack.get().asFile
        files.sync {
            from(design.resolve("tokens/VentiTheme.kt"))
            into(kotlin)
        }
        kotlin.get().asFile.resolve("VentiTokens.kt").writeText(tokens(design.resolve("tokens/tokens.json")))
        val out = res.get().asFile
        files.sync {
            // Android's resource names are lower case with underscores.
            from(design.resolve("fonts")) {
                include("*.ttf")
                into("font")
                rename { name -> name.removeSuffix(".ttf").replace(Regex("([a-z])([A-Z])"), "$1_$2").replace('-', '_').lowercase() + ".ttf" }
            }
            from(design.resolve("assets/app-icon/android/res"))
            into(out)
        }
        val drawables = out.resolve("drawable").apply { mkdirs() }
        design.resolve("assets/icons").listFiles { file -> file.extension == "svg" && file.name != "sprite.svg" }!!
            .forEach { svg ->
                drawables.resolve("venti_" + svg.nameWithoutExtension.replace('-', '_') + ".xml").writeText(vector(svg.readText()))
            }
        // The valley at the head of the Today screen, as the pack draws it.
        drawables.resolve("venti_art_valley_strip.xml")
            .writeText(art(design.resolve("assets/illustration/valley-strip.svg").readText()))
        // The window's colour before Compose draws, in each appearance.
        val theme = (groovy.json.JsonSlurper().parse(design.resolve("tokens/tokens.json")) as Map<*, *>)["theme"] as Map<*, *>
        for ((folder, name) in listOf("values" to "light", "values-night" to "dark")) {
            val bg = ((theme[name] as Map<*, *>)["bg"] as Map<*, *>)["\$value"]
            out.resolve(folder).apply { mkdirs() }.resolve("design_colours.xml").writeText(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <color name=\"venti_bg\">$bg</color>\n</resources>\n",
            )
        }
    }

    /** One of the pack's icons, 24 by 24 with round strokes of 2, as an Android vector drawable. */
    private fun vector(svg: String): String {
        fun attr(tag: String, name: String): String? = Regex("""\b$name="([^"]*)"""").find(tag)?.groupValues?.get(1)
        fun number(tag: String, name: String): Double = attr(tag, name)?.toDouble() ?: 0.0
        fun fmt(value: Double): String = value.toBigDecimal().stripTrailingZeros().toPlainString()
        val root = Regex("<svg[^>]*>").find(svg)!!.value
        val width = attr(root, "stroke-width") ?: "2"
        val paths = Regex("<(path|circle|ellipse)\\b[^>]*/>").findAll(svg).map { match ->
            val tag = match.value
            val data = when (match.groupValues[1]) {
                "path" -> attr(tag, "d")!!
                else -> {
                    val cx = number(tag, "cx")
                    val cy = number(tag, "cy")
                    val rx = attr(tag, "r")?.toDouble() ?: number(tag, "rx")
                    val ry = attr(tag, "r")?.toDouble() ?: number(tag, "ry")
                    "M${fmt(cx - rx)},${fmt(cy)}a${fmt(rx)},${fmt(ry)} 0 1,0 ${fmt(2 * rx)},0" +
                        "a${fmt(rx)},${fmt(ry)} 0 1,0 ${fmt(-2 * rx)},0Z"
                }
            }
            val fill = if (attr(tag, "fill") == "currentColor") "\n      android:fillColor=\"#FF000000\"" else ""
            "  <path\n      android:pathData=\"$data\"$fill\n      android:strokeColor=\"#FF000000\"\n" +
                "      android:strokeWidth=\"$width\"\n      android:strokeLineCap=\"round\"\n      android:strokeLineJoin=\"round\"/>\n"
        }.joinToString("")
        return "<!-- Made at build time from the design pack, design/assets/icons. -->\n" +
            "<vector xmlns:android=\"http://schemas.android.com/apk/res/android\"\n" +
            "    android:width=\"24dp\"\n    android:height=\"24dp\"\n" +
            "    android:viewportWidth=\"24\"\n    android:viewportHeight=\"24\">\n$paths</vector>\n"
    }

    /**
     * One of the pack's flat illustrations, shapes filled with a colour or
     * a straight gradient, as an Android vector drawable. Anything else in
     * it stops the build, so that the art is never drawn short.
     */
    private fun art(svg: String): String {
        fun attr(tag: String, name: String): String? = Regex("""\b$name="([^"]*)"""").find(tag)?.groupValues?.get(1)
        fun argb(hex: String): String = "#FF" + hex.removePrefix("#").uppercase()
        val box = attr(Regex("<svg[^>]*>").find(svg)!!.value, "viewBox")!!.split(' ').map { it.toDouble() }
        val (width, height) = box[2] to box[3]
        val gradients = Regex("""<linearGradient\b([^>]*)>(.*?)</linearGradient>""").findAll(svg).associate { match ->
            val tag = match.groupValues[1]
            require(attr(tag, "gradientUnits") == "userSpaceOnUse") { "only gradients in the picture's own units" }
            val stops = Regex("<stop\\b[^>]*/>").findAll(match.groupValues[2]).joinToString("") { stop ->
                "        <item android:offset=\"${attr(stop.value, "offset")}\" android:color=\"${argb(attr(stop.value, "stop-color")!!)}\"/>\n"
            }
            attr(tag, "id")!! to
                "    <aapt:attr name=\"android:fillColor\">\n      <gradient android:type=\"linear\"" +
                " android:startX=\"${attr(tag, "x1")}\" android:startY=\"${attr(tag, "y1")}\"" +
                " android:endX=\"${attr(tag, "x2")}\" android:endY=\"${attr(tag, "y2")}\">\n$stops      </gradient>\n    </aapt:attr>\n"
        }
        val body = svg.substringAfter("</defs>", svg.substringAfter(">")).removeSuffix("</svg>")
        val shapes = Regex("<(\\w+)\\b[^>]*/>").findAll(body).joinToString("") { match ->
            val tag = match.value
            val data = when (match.groupValues[1]) {
                "path" -> attr(tag, "d")!!
                "rect" -> "M0,0H${attr(tag, "width")}V${attr(tag, "height")}H0Z"
                else -> error("the art has a ${match.groupValues[1]}, which this does not draw")
            }
            val fill = attr(tag, "fill")!!
            val gradient = Regex("""url\(#(.+)\)""").matchEntire(fill)?.groupValues?.get(1)
            if (gradient != null) {
                "  <path android:pathData=\"$data\">\n${gradients.getValue(gradient)}  </path>\n"
            } else {
                "  <path android:pathData=\"$data\" android:fillColor=\"${argb(fill)}\"/>\n"
            }
        }
        return "<!-- Made at build time from the design pack, design/assets/illustration. -->\n" +
            "<vector xmlns:android=\"http://schemas.android.com/apk/res/android\"\n" +
            "    xmlns:aapt=\"http://schemas.android.com/aapt\"\n" +
            "    android:width=\"${(width / 2).toInt()}dp\"\n    android:height=\"${(height / 2).toInt()}dp\"\n" +
            "    android:viewportWidth=\"${width.toInt()}\"\n    android:viewportHeight=\"${height.toInt()}\">\n$shapes</vector>\n"
    }

    /**
     * The tokens `VentiTheme.kt` does not carry, as Kotlin: the type
     * styles, the sizes, the data scales with their breaks, airspace and
     * the map's colours.
     */
    private fun tokens(json: java.io.File): String {
        val all = groovy.json.JsonSlurper().parse(json) as Map<*, *>
        fun group(vararg path: String): Map<*, *> = path.fold(all as Any?) { at, key -> (at as Map<*, *>)[key] } as Map<*, *>
        fun value(token: Any?): Any? = (token as Map<*, *>)["\$value"]
        fun colour(token: Any?): String = "Color(0xFF${(value(token) as String).removePrefix("#").uppercase()})"
        fun px(text: Any?): String = (text as String).removeSuffix("px")
        fun camel(name: String): String = name.split('-').mapIndexed { i, part -> if (i == 0) part else part.replaceFirstChar(Char::uppercase) }.joinToString("")
        val out = StringBuilder()
        out.append("// Made at build time from design/tokens/tokens.json. Not edited by hand.\n")
        out.append("package app.venti.design\n\nimport androidx.compose.ui.graphics.Color\nimport androidx.compose.ui.unit.dp\n\n")
        out.append("data class VentiTypeStyle(val sizePx: Float, val lineHeightPx: Float, val weight: Int, val italic: Boolean, val letterSpacingEm: Float, val tabular: Boolean)\n\n")
        out.append("object VentiType {\n")
        for ((name, token) in group("type")) {
            val style = value(token) as Map<*, *>
            val extra = (((token as Map<*, *>)["\$extensions"] as Map<*, *>?)?.get("venti") as Map<*, *>?).orEmpty()
            val spacing = (style["letterSpacing"] as String?)?.removeSuffix("em") ?: "0"
            out.append("    val ${camel(name as String)} = VentiTypeStyle(${px(style["fontSize"])}f, ${px(style["lineHeight"])}f, ${style["fontWeight"]}, ${extra["italic"] ?: false}, ${spacing}f, ${extra["tabularNumbers"] ?: false})\n")
        }
        out.append("}\n\nobject VentiSize {\n")
        for ((name, token) in group("size")) out.append("    val ${camel(name as String)} = ${px(value(token))}.dp\n")
        out.append("}\n\nobject VentiMotion {\n")
        for ((name, token) in group("motion")) {
            val v = value(token)
            if (v is String) out.append("    const val ${camel(name as String)}Millis = ${v.removeSuffix("ms")}\n")
        }
        out.append("}\n\nobject VentiScaleBreaks {\n")
        for ((name, scale) in group("scale")) {
            val breaks = (((scale as Map<*, *>)["\$extensions"] as Map<*, *>)["venti"] as Map<*, *>)["breaks"] as List<*>
            out.append("    val ${camel(name as String)} = listOf(${breaks.joinToString { "${it}" + if (it.toString().contains('.')) "" else ".0" }})\n")
        }
        out.append("}\n\n")
        for ((kind, title) in listOf("airspace" to "VentiAirspace", "map" to "VentiMap")) {
            for (appearance in listOf("light", "dark")) {
                val name = title + appearance.replaceFirstChar(Char::uppercase)
                out.append("object $name {\n")
                for ((key, token) in group(kind, appearance)) out.append("    val ${camel(key as String)} = ${colour(token)}\n")
                out.append("}\n\n")
            }
        }
        return out.toString()
    }
}

val generated = layout.buildDirectory.dir("generated/venti")

val coreForAndroid = tasks.register<CoreForAndroid>("coreForAndroid") {
    description = "Builds the Rust core for Android."
    sources.from(coreSources)
    abis = coreAbis
    minSdk = android.defaultConfig.minSdk ?: error("minSdk is not set")
    repository = repositoryRoot
    ndk = androidComponents.sdkComponents.ndkDirectory
    jniLibs = generated.map { it.dir("jniLibs") }
}

val coreBindings = tasks.register<CoreBindings>("coreBindings") {
    description = "Generates the Kotlin interface of the Rust core."
    sources.from(coreSources)
    config = rootProject.layout.projectDirectory.file("uniffi.toml")
    repository = repositoryRoot
    kotlin = generated.map { it.dir("kotlin") }
    hostLibrary = generated.map { it.dir("host") }
}

val designPack = tasks.register<DesignPack>("designPack") {
    description = "Takes the design pack's tokens, fonts and icons into the build."
    pack = repositoryRoot.dir("design")
    kotlin = generated.map { it.dir("design-kotlin") }
    res = generated.map { it.dir("design-res") }
}

// The shared files live in the iOS app's folder, where they were first
// needed. They are read from there, not copied into the repository again.
val sharedAssets = tasks.register<SharedAssets>("sharedAssets") {
    description = "Gathers the sample flight, the map style and the bundled map region."
    sampleFlight = repositoryRoot.file("apps/ios/Venti/Resources/sample.igc")
    mapStyle = repositoryRoot.file("map/style.json")
    mapRegion = repositoryRoot.dir("apps/ios/Venti/Resources/MapDemo")
    fontLicence = repositoryRoot.file("design/fonts/OFL.txt")
    assets = generated.map { it.dir("assets") }
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(coreForAndroid, CoreForAndroid::jniLibs)
        variant.sources.kotlin?.addGeneratedSourceDirectory(coreBindings, CoreBindings::kotlin)
        variant.sources.assets?.addGeneratedSourceDirectory(sharedAssets, SharedAssets::assets)
        variant.sources.kotlin?.addGeneratedSourceDirectory(designPack, DesignPack::kotlin)
        variant.sources.res?.addGeneratedSourceDirectory(designPack, DesignPack::res)
    }
}

// The unit tests call the real core through the generated interface, and
// read the shared files from where the build gathered them. The wind map's
// tests play the server with the answers the server's own tests keep.
tasks.withType<Test>().configureEach {
    dependsOn(coreBindings, sharedAssets)
    val host = generated.map { it.dir("host").asFile.path }
    val assets = generated.map { it.dir("assets").asFile.path }
    val recordedAnswers = repositoryRoot.dir("crates/server/tests/golden").asFile
    inputs.dir(recordedAnswers).withPathSensitivity(PathSensitivity.RELATIVE)
    systemProperty("venti.recorded-answers", recordedAnswers.path)
    doFirst {
        systemProperty("jna.library.path", host.get())
        systemProperty("venti.assets", assets.get())
    }
}
