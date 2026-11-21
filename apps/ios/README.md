# Venti for iOS

The iPhone app: logbook, map with live wind, cockpit with vario and airspace warnings, and assistant, in SwiftUI over the Rust core. The cockpit has not flown yet: fly it only beside an instrument you trust.

## Build on a Mac

Needs Xcode 26 or later, Rust from [rustup.rs](https://rustup.rs) and Homebrew.

```bash
apps/ios/scripts/open-in-xcode.sh
```

That builds the Rust core, installs XcodeGen if it is missing, generates `Venti.xcodeproj` and opens it. Pick an iPhone simulator and press Run. After a change to the Rust core run `apps/ios/scripts/build-core.sh` again; after adding files or changing `project.yml`, run `xcodegen generate` in `apps/ios`.

The app talks to the Venti it was built with, `VENTI_SERVER` in `project.yml`: the Debug build to `http://127.0.0.1:8088` on the same Mac. To build it for another, change that value and run `xcodegen generate`; a Release build has no address yet and does not build.

## On your own iPhone

A real iPhone needs signing. The settings for it are personal and live in `Config/Local.xcconfig`, which git ignores; the setup script creates it from `Config/Local.example.xcconfig`, which explains the two ways: a free Apple ID (builds last seven days) or a certificate and provisioning profile you have.

Without a Mac: every push that touches the app builds it for a real iPhone without a signature, the `venti-unsigned-ipa` artifact of the `iOS` workflow. Sign it with your own certificate and a provisioning profile that lists your iPhone, and install the result.

## More

- [docs/design.md](../../docs/design.md): every screen, and how the app looks and why.
- [docs/architecture.md](../../docs/architecture.md#ios-first-target): how the app is built.
- [AGENTS.md](AGENTS.md): notes for AI sessions: the code's layout, the screenshots, what has never been tried on a phone.
