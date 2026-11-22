# Venti for iOS

The iPhone app, in SwiftUI over the Rust core. The cockpit has not flown yet: fly it only beside an instrument you trust.

Needs a Mac with Xcode 26 or later, Rust from [rustup.rs](https://rustup.rs) and Homebrew.

```bash
apps/ios/scripts/open-in-xcode.sh   # builds the core, generates Venti.xcodeproj and opens it
```

Pick an iPhone simulator and press Run. The Debug build talks to Venti at `http://127.0.0.1:8088` on the same Mac. A real iPhone needs signing: `Config/Local.example.xcconfig` explains the two ways.

- [docs/design.md](../../docs/design.md): every screen, and how the app looks and why.
- [docs/architecture.md](../../docs/architecture.md#ios-first-target): how the app is built.
- [AGENTS.md](AGENTS.md): working notes for AI sessions.
