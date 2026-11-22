# Venti for Android

The Android app, in Kotlin and Jetpack Compose over the Rust core.

Needs Java 17 or later, Rust from [rustup.rs](https://rustup.rs) with `rustup target add aarch64-linux-android x86_64-linux-android`, `cargo install cargo-ndk --version 4.1.2 --locked`, and the Android SDK with platform 37, build tools 36.0.0 and NDK 29.0.14206865, which Android Studio installs when it opens the project (`ANDROID_HOME` points at the SDK).

```bash
cd apps/android
./gradlew :app:installDebug   # builds the core and the app, and puts it on a connected phone or a running emulator
```

A debug build talks to Venti at `http://10.0.2.2:8088`, the computer as an emulator reaches it; `-Pventi.server=https://…` builds it for another address.

- [docs/design.md](../../docs/design.md#android): how the app looks and where it differs from the iPhone's.
- [docs/architecture.md](../../docs/architecture.md#android): how it is built.
- [AGENTS.md](AGENTS.md): working notes for AI sessions.
