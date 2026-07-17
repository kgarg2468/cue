// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CaptureDelegate",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "CaptureDelegateIPC", targets: ["CaptureDelegateIPC"]),
        .executable(name: "capture-delegate-health", targets: ["CaptureDelegateHealth"]),
        .executable(name: "CaptureDelegateApp", targets: ["CaptureDelegateApp"]),
    ],
    targets: [
        .target(name: "CaptureDelegateIPC"),
        .executableTarget(
            name: "CaptureDelegateHealth",
            dependencies: ["CaptureDelegateIPC"]
        ),
        .executableTarget(
            name: "CaptureDelegateApp",
            dependencies: ["CaptureDelegateIPC"]
        ),
        .testTarget(
            name: "CaptureDelegateIPCTests",
            dependencies: ["CaptureDelegateIPC"]
        ),
        .testTarget(
            name: "CaptureDelegateIntegrationTests",
            dependencies: ["CaptureDelegateIPC"]
        ),
    ]
)
