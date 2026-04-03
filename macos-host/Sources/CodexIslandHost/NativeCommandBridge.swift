import Foundation

enum NativeBridgeCommandError: Error {
    case missingExecutable
    case invalidUtf8
    case commandFailed(String)
}

extension NativeBridgeCommandError: LocalizedError {
    var errorDescription: String? {
        switch self {
        case .missingExecutable:
            return "Native bridge executable is missing"
        case .invalidUtf8:
            return "Native bridge returned invalid text"
        case let .commandFailed(message):
            return message
        }
    }
}

final class NativeCommandBridge {
    private let bundle: Bundle
    private let fileManager: FileManager
    private let currentDirectoryURL: URL

    init(
        bundle: Bundle = .main,
        fileManager: FileManager = .default,
        currentDirectoryURL: URL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    ) {
        self.bundle = bundle
        self.fileManager = fileManager
        self.currentDirectoryURL = currentDirectoryURL
    }

    func getSessionsJSON() throws -> String {
        try run(arguments: ["get-sessions"])
    }

    func focusSession(sessionId: String) throws {
        _ = try run(arguments: ["focus-session", sessionId])
    }

    func submitSessionReply(sessionId: String, reply: String) throws {
        _ = try run(arguments: ["submit-session-reply", sessionId, reply])
    }

    private func run(arguments: [String]) throws -> String {
        let executable = RuntimePaths.nativeBridgeExecutableURL(
            bundle: bundle,
            fileManager: fileManager,
            currentDirectoryURL: currentDirectoryURL
        )

        guard fileManager.isExecutableFile(atPath: executable.path) else {
            throw NativeBridgeCommandError.missingExecutable
        }

        let process = Process()
        process.executableURL = executable
        process.arguments = arguments

        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr

        try process.run()
        process.waitUntilExit()

        let outputData = stdout.fileHandleForReading.readDataToEndOfFile()
        let errorData = stderr.fileHandleForReading.readDataToEndOfFile()

        guard process.terminationStatus == 0 else {
            let errorText = String(data: errorData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let fallbackOutput = String(data: outputData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let message = if let errorText, !errorText.isEmpty {
                errorText
            } else if let fallbackOutput, !fallbackOutput.isEmpty {
                fallbackOutput
            } else {
                "bridge command failed"
            }
            throw NativeBridgeCommandError.commandFailed(message)
        }

        guard let output = String(data: outputData, encoding: .utf8) else {
            throw NativeBridgeCommandError.invalidUtf8
        }

        return output.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
