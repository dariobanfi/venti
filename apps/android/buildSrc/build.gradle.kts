// What the app's build script uses beyond Gradle's own: plain Java with
// nothing to download. The app's unit tests compile the same sources
// (`app/build.gradle.kts`), so what is here is tested there.
plugins {
    java
}

tasks.withType<JavaCompile>().configureEach {
    options.release = 17
}
