// swift-tools-version: 6.0
import PackageDescription

// The Rust core, as a Swift package.
//
// `VentiCoreFFI.xcframework` and `Sources/VentiCore/Generated` are build
// products: run `apps/ios/scripts/build-core.sh` to create them.
let package = Package(
    name: "VentiCore",
    platforms: [.iOS(.v18), .macOS(.v14)],
    products: [
        .library(name: "VentiCore", targets: ["VentiCore"]),
    ],
    targets: [
        .binaryTarget(name: "VentiCoreFFI", path: "VentiCoreFFI.xcframework"),
        .target(name: "VentiCore", dependencies: ["VentiCoreFFI"]),
        .testTarget(name: "VentiCoreTests", dependencies: ["VentiCore"]),
    ]
)
