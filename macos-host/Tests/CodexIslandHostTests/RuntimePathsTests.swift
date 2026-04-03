import Foundation
import XCTest
@testable import CodexIslandHost

final class RuntimePathsTests: XCTestCase {
    func testDistURLPrefersBundledResources() throws {
        let tempRoot = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bundleURL = tempRoot.appendingPathComponent("Codex Island.app", isDirectory: true)
        let resourcesURL = bundleURL.appendingPathComponent("Contents/Resources", isDirectory: true)
        let distURL = resourcesURL.appendingPathComponent("dist", isDirectory: true)
        let cwdURL = tempRoot.appendingPathComponent("workspace", isDirectory: true)

        try FileManager.default.createDirectory(at: distURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: cwdURL.appendingPathComponent("dist", isDirectory: true),
            withIntermediateDirectories: true
        )

        let bundle = Bundle(url: bundleURL)
        let resolved = RuntimePaths.distURL(
            bundle: bundle ?? .main,
            currentDirectoryURL: cwdURL
        )

        XCTAssertEqual(resolved.standardizedFileURL, distURL.standardizedFileURL)
    }

    func testDistURLFallsBackToCurrentDirectory() throws {
        let cwdURL = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let distURL = cwdURL.appendingPathComponent("dist", isDirectory: true)
        try FileManager.default.createDirectory(at: distURL, withIntermediateDirectories: true)

        let resolved = RuntimePaths.distURL(currentDirectoryURL: cwdURL)

        XCTAssertEqual(resolved.standardizedFileURL, distURL.standardizedFileURL)
    }

    func testNativeBridgeExecutablePrefersBundledBinary() throws {
        let tempRoot = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bundleURL = tempRoot.appendingPathComponent("Codex Island.app", isDirectory: true)
        let resourcesURL = bundleURL.appendingPathComponent("Contents/Resources/bin", isDirectory: true)
        let bridgeURL = resourcesURL.appendingPathComponent("codex-island-native-bridge")
        let cwdURL = tempRoot.appendingPathComponent("workspace", isDirectory: true)

        try FileManager.default.createDirectory(at: resourcesURL, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: bridgeURL.path, contents: Data())
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: bridgeURL.path
        )

        let bundle = Bundle(url: bundleURL)
        let resolved = RuntimePaths.nativeBridgeExecutableURL(
            bundle: bundle ?? .main,
            currentDirectoryURL: cwdURL
        )

        XCTAssertEqual(resolved.standardizedFileURL, bridgeURL.standardizedFileURL)
    }
}
