import Foundation

enum RuntimePaths {
    static func distURL(
        bundle: Bundle = .main,
        fileManager: FileManager = .default,
        currentDirectoryURL: URL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    ) -> URL {
        let candidates = [
            bundle.resourceURL?.appendingPathComponent("dist", isDirectory: true),
            currentDirectoryURL.appendingPathComponent("dist", isDirectory: true),
        ]

        for candidate in candidates.compactMap({ $0 }) {
            var isDirectory: ObjCBool = false
            if fileManager.fileExists(atPath: candidate.path, isDirectory: &isDirectory),
               isDirectory.boolValue {
                return candidate
            }
        }

        return candidates.compactMap({ $0 }).first ?? currentDirectoryURL
    }

    static func nativeBridgeExecutableURL(
        bundle: Bundle = .main,
        fileManager: FileManager = .default,
        currentDirectoryURL: URL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    ) -> URL {
        let candidates = [
            bundle.resourceURL?.appendingPathComponent("bin/codex-island-native-bridge"),
            currentDirectoryURL
                .appendingPathComponent("native-bridge", isDirectory: true)
                .appendingPathComponent("target", isDirectory: true)
                .appendingPathComponent("debug", isDirectory: true)
                .appendingPathComponent("codex-island-native-bridge"),
            currentDirectoryURL
                .appendingPathComponent("native-bridge", isDirectory: true)
                .appendingPathComponent("target", isDirectory: true)
                .appendingPathComponent("release", isDirectory: true)
                .appendingPathComponent("codex-island-native-bridge"),
        ]

        for candidate in candidates.compactMap({ $0 }) {
            if fileManager.isExecutableFile(atPath: candidate.path) {
                return candidate
            }
        }

        return candidates.compactMap({ $0 }).first ?? currentDirectoryURL
    }
}
