// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "CodexIslandHost",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "CodexIslandHost", targets: ["CodexIslandHost"]),
        .executable(name: "CodexIslandHostApp", targets: ["CodexIslandHostApp"])
    ],
    targets: [
        .target(name: "CodexIslandHost"),
        .executableTarget(
            name: "CodexIslandHostApp",
            dependencies: ["CodexIslandHost"]
        ),
        .testTarget(
            name: "CodexIslandHostTests",
            dependencies: ["CodexIslandHost"]
        )
    ]
)
