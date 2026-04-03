import AppKit
import Foundation
import WebKit

protocol WebViewBridgeDelegate: AnyObject {
    func syncIslandWindow(expanded: Bool, expandedView: ExpandedView, sessionCount: Int)
}

private final class AppAssetSchemeHandler: NSObject, WKURLSchemeHandler {
    private let distURL: URL

    init(distURL: URL) {
        self.distURL = distURL
    }

    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        guard let requestURL = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(NSError(domain: NSURLErrorDomain, code: NSURLErrorBadURL))
            return
        }

        let requestPath = requestURL.path.isEmpty || requestURL.path == "/" ? "/index.html" : requestURL.path
        let relativePath = String(requestPath.drop(while: { $0 == "/" }))
        let fileURL = distURL.appendingPathComponent(relativePath)

        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            WebViewBridge.log("scheme missing asset: \(relativePath)")
            urlSchemeTask.didFailWithError(
                NSError(domain: NSURLErrorDomain, code: NSURLErrorFileDoesNotExist)
            )
            return
        }

        do {
            let data = try Data(contentsOf: fileURL)
            let response = URLResponse(
                url: requestURL,
                mimeType: Self.mimeType(for: fileURL.pathExtension),
                expectedContentLength: data.count,
                textEncodingName: Self.textEncoding(for: fileURL.pathExtension)
            )
            urlSchemeTask.didReceive(response)
            urlSchemeTask.didReceive(data)
            urlSchemeTask.didFinish()
        } catch {
            WebViewBridge.log("scheme asset read failed: \(relativePath) \(error.localizedDescription)")
            urlSchemeTask.didFailWithError(error)
        }
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {}

    private static func mimeType(for pathExtension: String) -> String {
        switch pathExtension.lowercased() {
        case "html":
            return "text/html"
        case "js", "mjs":
            return "text/javascript"
        case "css":
            return "text/css"
        case "json":
            return "application/json"
        case "svg":
            return "image/svg+xml"
        case "png":
            return "image/png"
        case "jpg", "jpeg":
            return "image/jpeg"
        case "woff":
            return "font/woff"
        case "woff2":
            return "font/woff2"
        default:
            return "application/octet-stream"
        }
    }

    private static func textEncoding(for pathExtension: String) -> String? {
        switch pathExtension.lowercased() {
        case "html", "js", "mjs", "css", "json", "svg":
            return "utf-8"
        default:
            return nil
        }
    }
}

final class WebViewLogger: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        WebViewBridge.log("webview didFinish")
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            webView.evaluateJavaScript(
                """
                ({
                  href: window.location.href,
                  readyState: document.readyState,
                  rootChildren: document.getElementById('root')?.children.length ?? -1,
                  rootHtmlLength: document.getElementById('root')?.innerHTML?.length ?? -1,
                  bodyTextLength: document.body?.innerText?.length ?? -1
                })
                """
            ) { value, error in
                if let error {
                    WebViewBridge.log("dom inspect error: \(error.localizedDescription)")
                    return
                }

                WebViewBridge.log("dom inspect: \(String(describing: value))")
            }
        }
    }

    func webView(
        _ webView: WKWebView,
        didFail navigation: WKNavigation!,
        withError error: Error
    ) {
        WebViewBridge.log("webview didFail: \(error.localizedDescription)")
    }

    func webView(
        _ webView: WKWebView,
        didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        WebViewBridge.log("webview provisional fail: \(error.localizedDescription)")
    }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        WebViewBridge.log("webview console: \(message.body)")
    }
}

private final class NativeBridgeHandler: NSObject, WKScriptMessageHandler {
    weak var delegate: WebViewBridgeDelegate?
    private let commandBridge = NativeCommandBridge()

    init(delegate: WebViewBridgeDelegate?) {
        self.delegate = delegate
    }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        guard
            let body = message.body as? [String: Any],
            let kind = body["kind"] as? String
        else {
            WebViewBridge.log("native bridge received malformed message")
            return
        }

        switch kind {
        case "getSessions":
            WebViewBridge.log("native bridge request: getSessions")
            guard let requestId = body["requestId"] as? String else {
                WebViewBridge.log("native bridge missing requestId for getSessions")
                return
            }
            respond(
                requestId: requestId,
                command: {
                    try self.commandBridge.getSessionsJSON()
                }
            )
        case "focusSession":
            WebViewBridge.log("native bridge request: focusSession")
            guard
                let requestId = body["requestId"] as? String,
                let payload = body["payload"] as? [String: Any],
                let sessionId = payload["sessionId"] as? String
            else {
                WebViewBridge.log("native bridge focus payload invalid")
                return
            }
            respond(
                requestId: requestId,
                command: {
                    try self.commandBridge.focusSession(sessionId: sessionId)
                    return "null"
                }
            )
        case "submitSessionReply":
            WebViewBridge.log("native bridge request: submitSessionReply")
            guard
                let requestId = body["requestId"] as? String,
                let payload = body["payload"] as? [String: Any],
                let sessionId = payload["sessionId"] as? String,
                let reply = payload["reply"] as? String
            else {
                WebViewBridge.log("native bridge submit payload invalid")
                return
            }
            respond(
                requestId: requestId,
                command: {
                    try self.commandBridge.submitSessionReply(sessionId: sessionId, reply: reply)
                    return "null"
                }
            )
        case "syncIslandWindow":
            guard
                let payload = body["payload"] as? [String: Any],
                let expanded = payload["expanded"] as? Bool,
                let expandedViewRaw = payload["expandedView"] as? String,
                let expandedView = ExpandedView(rawValue: expandedViewRaw),
                let sessionCount = payload["sessionCount"] as? Int
            else {
                WebViewBridge.log("native bridge sync payload invalid")
                return
            }

            delegate?.syncIslandWindow(
                expanded: expanded,
                expandedView: expandedView,
                sessionCount: sessionCount
            )
        default:
            WebViewBridge.log("native bridge unknown kind: \(kind)")
        }
    }

    private func respond(requestId: String, command: @escaping () throws -> String) {
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let payloadJSON = try command()
                WebViewBridge.log("native bridge response ok: \(requestId)")
                WebViewBridge.respondToRequest(
                    requestId: requestId,
                    success: true,
                    payloadJSON: payloadJSON
                )
            } catch {
                WebViewBridge.log("native bridge response failed: \(requestId) \(error.localizedDescription)")
                WebViewBridge.respondToRequest(
                    requestId: requestId,
                    success: false,
                    payloadJSON: Self.errorPayloadJSON(message: error.localizedDescription)
                )
            }
        }
    }

    private static func errorPayloadJSON(message: String) -> String {
        let object = ["message": message]
        let data = try? JSONSerialization.data(withJSONObject: object)
        return String(data: data ?? Data("{\"message\":\"unknown error\"}".utf8), encoding: .utf8)
            ?? "{\"message\":\"unknown error\"}"
    }
}

public final class WebViewBridge {
    private let logger = WebViewLogger()
    private let distURL: URL
    private let schemeHandler: AppAssetSchemeHandler
    private let nativeBridgeHandler: NativeBridgeHandler
    private weak var webView: WKWebView?

    init(delegate: WebViewBridgeDelegate?) {
        self.distURL = RuntimePaths.distURL()
        self.schemeHandler = AppAssetSchemeHandler(distURL: distURL)
        self.nativeBridgeHandler = NativeBridgeHandler(delegate: delegate)
    }

    public func makeContainer(frame: NSRect) -> NSView {
        let container = NSView(frame: frame)
        container.wantsLayer = true
        container.layer?.backgroundColor = NSColor.clear.cgColor

        let webView = makeWebView(frame: frame)
        self.webView = webView
        webView.autoresizingMask = [.width, .height]
        webView.frame = container.bounds
        container.addSubview(webView)
        return container
    }

    private func makeWebView(frame: NSRect) -> WKWebView {
        let controller = WKUserContentController()
        let bootstrap = WKUserScript(
            source: """
            (function () {
              let nextRequestId = 0;
              const pending = new Map();
              function post(kind, payload) {
                const requestId = "native-" + String(++nextRequestId);
                window.webkit.messageHandlers.codexLogger.postMessage(
                  "post " + kind + " " + requestId + " " + JSON.stringify(payload || null)
                );
                window.webkit.messageHandlers.codexBridge.postMessage({
                  kind,
                  requestId,
                  payload
                });
                return new Promise((resolve, reject) => {
                  pending.set(requestId, { resolve, reject });
                });
              }

              window.__resolveCodexNativeRequest = function (requestId, success, payload) {
                const pendingRequest = pending.get(requestId);
                if (!pendingRequest) {
                  return;
                }
                pending.delete(requestId);
                if (success) {
                  pendingRequest.resolve(payload);
                } else {
                  const message = payload && payload.message ? payload.message : "Native host request failed";
                  pendingRequest.reject(new Error(message));
                }
              };

            window.__CODEX_ISLAND_NATIVE__ = {
              getSessions() {
                return post("getSessions").then((payload) => payload);
              },
              focusSession(sessionId) {
                return post("focusSession", { sessionId });
              },
              submitSessionReply(sessionId, reply) {
                return post("submitSessionReply", { sessionId, reply });
              },
              syncIslandWindow(payload) {
                window.webkit.messageHandlers.codexBridge.postMessage({
                  kind: "syncIslandWindow",
                  payload
                });
                return Promise.resolve();
              },
              listenSessionsUpdated(listener) {
                let disposed = false;
                let lastSerialized = "";

                const poll = function () {
                  if (disposed) {
                    return;
                  }

                  window.__CODEX_ISLAND_NATIVE__.getSessions()
                    .then(function (payload) {
                      const serialized = JSON.stringify(payload);
                      if (serialized !== lastSerialized) {
                        lastSerialized = serialized;
                        listener(payload);
                      }
                    })
                    .catch(function (error) {
                      console.error("native poll failed", error);
                    })
                    .finally(function () {
                      if (!disposed) {
                        window.setTimeout(poll, 2000);
                      }
                    });
                };

                poll();
                return function () {
                  disposed = true;
                };
              }
            };
            window.addEventListener("codex-island:set-hover", function (event) {
              window.dispatchEvent(new CustomEvent("codex-island:hover", {
                detail: Boolean(event.detail)
              }));
            });
            window.addEventListener("error", function (event) {
              window.webkit.messageHandlers.codexLogger.postMessage(
                "window.error: " + event.message + " @ " + event.filename + ":" + event.lineno
              );
            });
            window.addEventListener("unhandledrejection", function (event) {
              window.webkit.messageHandlers.codexLogger.postMessage(
                "unhandledrejection: " + String(event.reason)
              );
            });
            const originalLog = console.log;
            console.log = function () {
              window.webkit.messageHandlers.codexLogger.postMessage(
                "console.log: " + Array.from(arguments).map(String).join(" ")
              );
              return originalLog.apply(console, arguments);
            };
            const originalError = console.error;
            console.error = function () {
              window.webkit.messageHandlers.codexLogger.postMessage(
                "console.error: " + Array.from(arguments).map(String).join(" ")
              );
              return originalError.apply(console, arguments);
            };
            })();
            """,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        )
        controller.addUserScript(bootstrap)
        controller.add(logger, name: "codexLogger")
        controller.add(nativeBridgeHandler, name: "codexBridge")

        let configuration = WKWebViewConfiguration()
        configuration.userContentController = controller
        configuration.setURLSchemeHandler(schemeHandler, forURLScheme: "codex-island")

        let webView = WKWebView(frame: frame, configuration: configuration)
        webView.setValue(false, forKey: "drawsBackground")
        webView.navigationDelegate = logger
        webView.wantsLayer = true
        loadApp(into: webView)
        return webView
    }

    private func loadApp(into webView: WKWebView) {
        let indexURL = distURL.appendingPathComponent("index.html")
        let rootURL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        Self.log("cwd: \(rootURL.path)")
        Self.log("index path: \(indexURL.path)")

        guard FileManager.default.fileExists(atPath: indexURL.path) else {
            Self.log("missing dist/index.html")
            webView.loadHTMLString(
                """
                <html>
                  <body style="margin:0;background:#111;color:white;font:16px -apple-system;padding:20px;">
                    Missing dist/index.html. Run <code>pnpm build</code>.
                  </body>
                </html>
                """,
                baseURL: nil
            )
            return
        }

        guard let appURL = URL(string: "codex-island://app/index.html") else {
            Self.log("failed to build app url")
            return
        }

        Self.log("loading app url: \(appURL.absoluteString)")
        webView.load(URLRequest(url: appURL))
    }

    func emitHoverChanged(_ hovering: Bool) {
        let script = """
        window.dispatchEvent(new CustomEvent("codex-island:set-hover", { detail: \(hovering ? "true" : "false") }));
        """
        DispatchQueue.main.async { [weak self] in
            self?.webView?.evaluateJavaScript(script) { _, error in
                if let error {
                    Self.log("emit hover failed: \(error.localizedDescription)")
                }
            }
        }
    }

    func evaluateJavaScript(_ script: String) {
        DispatchQueue.main.async { [weak self] in
            self?.webView?.evaluateJavaScript(script) { _, error in
                if let error {
                    Self.log("evaluate js failed: \(error.localizedDescription)")
                }
            }
        }
    }

    static func log(_ message: String) {
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

    static func respondToRequest(requestId: String, success: Bool, payloadJSON: String) {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate,
              let panel = appDelegate.panel else {
            log("respond request failed: missing panel")
            return
        }

        let requestIdLiteral = jsLiteral(requestId)
        let script = """
        window.__resolveCodexNativeRequest(\(requestIdLiteral), \(success ? "true" : "false"), \(payloadJSON));
        """
        DispatchQueue.main.async {
            panel.evaluateJavaScript(script)
        }
    }

    private static func jsLiteral(_ value: String) -> String {
        let data = try? JSONSerialization.data(withJSONObject: [value])
        let encoded = String(data: data ?? Data("[\"\"]".utf8), encoding: .utf8) ?? "[\"\"]"
        return String(encoded.dropFirst().dropLast())
    }
}
