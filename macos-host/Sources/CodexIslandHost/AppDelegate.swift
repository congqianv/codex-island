import AppKit
import Foundation

public final class AppDelegate: NSObject, NSApplicationDelegate {
    public var panel: IslandPanel?
    private let nativeBridge = NativeCommandBridge()

    public override init() {
        super.init()
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        log("applicationDidFinishLaunching")
        DispatchQueue.global(qos: .utility).async {
            do {
                try self.nativeBridge.ensureHooks()
                self.log("managed hooks ensured")
            } catch {
                self.log("managed hooks ensure failed: \(error.localizedDescription)")
            }
        }
        panel = IslandPanel()
        panel?.positionOnPrimaryScreen()
        panel?.makeKeyAndOrderFront(nil)
        panel?.orderFrontRegardless()
        log("panel visible: \(panel?.isVisible ?? false)")
        log("panel frame: \(NSStringFromRect(panel?.frame ?? .zero))")
        NSApp.activate(ignoringOtherApps: true)
    }

    private func log(_ message: String) {
        let path = "/tmp/codex-island-host.log"
        let line = "[CodexIslandHost] \(message)\n"
        if let data = line.data(using: .utf8) {
            if FileManager.default.fileExists(atPath: path),
               let handle = FileHandle(forWritingAtPath: path) {
                handle.seekToEndOfFile()
                try? handle.write(contentsOf: data)
                try? handle.close()
            } else {
                FileManager.default.createFile(atPath: path, contents: data)
            }
        }
    }
}
