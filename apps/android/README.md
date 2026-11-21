# Venti for Android

The Android app: logbook, map with live wind, cockpit and assistant, in Kotlin and Jetpack Compose over the Rust core. It has the iPhone app's sections and has run on an emulator only, never on a phone.

## Install without building

Every push that touches the app builds a debug copy, the `venti-debug-apk` artifact of the `Android` workflow. With `gh` and `adb`:

```bash
gh run download --name venti-debug-apk --dir /tmp/venti   # the newest run
adb install -r /tmp/venti/app-debug.apk
```

It runs on 64-bit phones with Android 8 or later. Each build machine signs with a throwaway key of its own, so a build from another machine does not install over it: remove the app first, and its logbook with it.

## Build

Needs:

- Java 17 or later.
- Rust from [rustup.rs](https://rustup.rs), with `rustup target add aarch64-linux-android x86_64-linux-android`.
- `cargo install cargo-ndk --version 4.1.2 --locked`.
- The Android SDK with platform 37, build tools 36.0.0 and NDK 29.0.14206865. Android Studio installs them when it opens the project; without it, `sdkmanager "platforms;android-37.0" "build-tools;36.0.0" "ndk;29.0.14206865"`, with `ANDROID_HOME` (or `sdk.dir` in `apps/android/local.properties`) pointing at the SDK.

Then, in `apps/android`:

```bash
./gradlew :app:assembleDebug               # app/build/outputs/apk/debug/app-debug.apk
./gradlew :app:installDebug                # onto a connected phone or a running emulator
./gradlew :app:testDebugUnitTest           # unit tests, against the real core
./gradlew :app:lintDebug
./gradlew :app:connectedDebugAndroidTest   # UI tests, on a phone or an emulator
```

Gradle builds the Rust core and its Kotlin interface by itself.

The app talks to the Venti it was built for: `-Pventi.server=https://…` builds it for another address. A debug build without it talks to `http://10.0.2.2:8088`, Venti on the computer as an emulator reaches it; a release build needs the address.

## More

- [docs/design.md](../../docs/design.md#android): how the app looks and where it differs from the iPhone's.
- [docs/architecture.md](../../docs/architecture.md#android): how it is built, and what is still to verify on a phone.
- [AGENTS.md](AGENTS.md): notes for AI sessions: checks, the UI tests and their pitfalls, the code's layout, what has never been tried.
