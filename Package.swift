// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "yesyesyes",
    platforms: [.macOS(.v13)],
    targets: [
        .target(name: "YesYesYesCore"),
        .executableTarget(name: "yesyesyes", dependencies: ["YesYesYesCore"]),
        // Plain runner: XCTest and Swift Testing are not part of the Command Line Tools.
        .executableTarget(name: "selftest", dependencies: ["YesYesYesCore"]),
    ]
)
