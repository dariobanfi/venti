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
    fileTree(repositoryRoot.dir("crates")) { include("core/**", "flight/**", "sensors/**", "domain/**") },
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

    @get:OutputDirectory abstract val assets: DirectoryProperty

    @TaskAction
    fun gather() {
        files.sync {
            from(sampleFlight)
            from(mapStyle)
            // The fonts and the demo region keep their folder: the map
            // engine asks for fonts by folder and file name.
            from(mapRegion) { into("MapDemo") }
            into(assets)
        }
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

// The shared files live in the iOS app's folder, where they were first
// needed. They are read from there, not copied into the repository again.
val sharedAssets = tasks.register<SharedAssets>("sharedAssets") {
    description = "Gathers the sample flight, the map style and the bundled map region."
    sampleFlight = repositoryRoot.file("apps/ios/Venti/Resources/sample.igc")
    mapStyle = repositoryRoot.file("map/style.json")
    mapRegion = repositoryRoot.dir("apps/ios/Venti/Resources/MapDemo")
    assets = generated.map { it.dir("assets") }
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(coreForAndroid, CoreForAndroid::jniLibs)
        variant.sources.kotlin?.addGeneratedSourceDirectory(coreBindings, CoreBindings::kotlin)
        variant.sources.assets?.addGeneratedSourceDirectory(sharedAssets, SharedAssets::assets)
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
