// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "HiveTest",
    platforms: [
        .iOS(.v16),
        .macOS(.v13)
    ],
    products: [
        .executable(name: "HiveTest", targets: ["HiveTest"]),
    ],
    dependencies: [],
    targets: [
        // Binary target for the Rust FFI library
        .binaryTarget(
            name: "HiveFFI",
            path: "build/HiveFFI.xcframework"
        ),
        .executableTarget(
            name: "HiveTest",
            dependencies: [
                "HiveFFI",
            ],
            path: "HiveTest",
            exclude: ["Info.plist"],
            linkerSettings: [
                .linkedFramework("CoreBluetooth"),
            ]
        ),
    ]
)
