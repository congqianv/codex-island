import AppKit
import Foundation

public final class IslandPanel: NSPanel {
    private static let collapsedHoverWidth: CGFloat = 220
    private var layoutMetrics = islandLayoutMetrics(expanded: false, expandedView: .list, sessionCount: 0)
    private lazy var webViewBridge = WebViewBridge(delegate: self)
    private var hoverTimer: Timer?
    private var hoverActive = false
    private var isExpanded = false

    public init() {
        Self.log("init start")
        super.init(
            contentRect: NSRect(
                origin: .zero,
                size: NSSize(width: layoutMetrics.width, height: layoutMetrics.height)
            ),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        Self.log("after super.init")

        titleVisibility = .hidden
        Self.log("titleVisibility")
        titlebarAppearsTransparent = true
        Self.log("titlebarAppearsTransparent")
        isFloatingPanel = true
        Self.log("isFloatingPanel")
        hidesOnDeactivate = false
        Self.log("hidesOnDeactivate")
        hasShadow = false
        Self.log("hasShadow")
        isOpaque = false
        Self.log("isOpaque")
        backgroundColor = .clear
        Self.log("backgroundColor")
        level = .screenSaver
        Self.log("level")
        collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary,
        ]
        Self.log("collectionBehavior")
        animationBehavior = .none
        Self.log("animationBehavior")
        isMovable = false
        Self.log("isMovable")
        isMovableByWindowBackground = false
        Self.log("isMovableByWindowBackground")
        becomesKeyOnlyIfNeeded = false
        Self.log("becomesKeyOnlyIfNeeded")
        worksWhenModal = true
        Self.log("worksWhenModal")
        isReleasedWhenClosed = false
        Self.log("isReleasedWhenClosed")
        contentView = webViewBridge.makeContainer(
            frame: NSRect(
                origin: .zero,
                size: NSSize(width: layoutMetrics.width, height: layoutMetrics.height)
            )
        )
        Self.log("contentView")
        startHoverMonitor()
    }

    public override var canBecomeKey: Bool { true }

    public override var canBecomeMain: Bool { true }

    public func positionOnPrimaryScreen() {
        let screenFrame = NSScreen.main?.frame ?? frame
        let origin = NSPoint(
            x: screenFrame.midX - (layoutMetrics.width / 2),
            y: screenFrame.maxY - layoutMetrics.height
        )

        setFrameOrigin(origin)
    }

    func syncLayout(expanded: Bool, expandedView: ExpandedView, sessionCount: Int) {
        isExpanded = expanded
        layoutMetrics = islandLayoutMetrics(
            expanded: expanded,
            expandedView: expandedView,
            sessionCount: sessionCount
        )
        let screenFrame = NSScreen.main?.frame ?? frame
        let nextFrame = NSRect(
            x: screenFrame.midX - (layoutMetrics.width / 2),
            y: screenFrame.maxY - layoutMetrics.height,
            width: layoutMetrics.width,
            height: layoutMetrics.height
        )

        setFrame(nextFrame, display: true)
        contentView?.frame = NSRect(origin: .zero, size: nextFrame.size)
        orderFrontRegardless()
    }

    private func startHoverMonitor() {
        hoverTimer?.invalidate()
        hoverTimer = Timer.scheduledTimer(withTimeInterval: 0.08, repeats: true) { [weak self] _ in
            self?.updateHoverState()
        }
        RunLoop.main.add(hoverTimer!, forMode: .common)
    }

    private func updateHoverState() {
        let mouseLocation = NSEvent.mouseLocation
        let hovering = hoverRegion().contains(mouseLocation)

        guard hovering != hoverActive else {
            return
        }

        hoverActive = hovering
        Self.log("hover changed: \(hovering)")
        webViewBridge.emitHoverChanged(hovering)
    }

    private func hoverRegion() -> NSRect {
        if isExpanded {
            return frame
        }

        let collapsedMetrics = islandLayoutMetrics(
            expanded: false,
            expandedView: .list,
            sessionCount: 0
        )
        let origin = NSPoint(
            x: frame.midX - (Self.collapsedHoverWidth / 2),
            y: frame.maxY - collapsedMetrics.height
        )
        return NSRect(
            origin: origin,
            size: NSSize(width: Self.collapsedHoverWidth, height: collapsedMetrics.height)
        )
    }

    func evaluateJavaScript(_ script: String) {
        webViewBridge.evaluateJavaScript(script)
    }

    private static func log(_ message: String) {
        let path = "/tmp/codex-island-host.log"
        let line = "[CodexIslandHost] panel \(message)\n"
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

extension IslandPanel: WebViewBridgeDelegate {
    func syncIslandWindow(expanded: Bool, expandedView: ExpandedView, sessionCount: Int) {
        Self.log(
            "sync layout expanded=\(expanded) view=\(expandedView.rawValue) sessions=\(sessionCount)"
        )
        DispatchQueue.main.async { [weak self] in
            self?.syncLayout(
                expanded: expanded,
                expandedView: expandedView,
                sessionCount: sessionCount
            )
        }
    }
}
