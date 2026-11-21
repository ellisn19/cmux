import AppKit
import CmuxNextPagesTestSupport
import CmuxNextTestSupport
import Testing
@testable import CmuxNextAgentPane
@testable import CmuxNextPages

/// A key the New Tab key hold kept reaches the New Tab field also while cmux is not the active app.
///
/// With the app not active no window is key, and Chromium drops a key event sent to a page window
/// that is not key: the page's document has no focus. The field already has the caret, so on the
/// fleet (`CMUX_NEXT_NO_ACTIVATE`, a script's or a background Cmd-T) the first key, a `/` that opens
/// the slash menu included, was lost in 24 of 24 openings. The key's text goes in through the page's
/// input bridge instead, after the text of the keys before it.
@Suite(.serialized, .closesItsWindows)
@MainActor
struct HeldKeyWhileAppInactiveTests {
    private actor NoHost: AgentPaneHostProviding {
        func handshake(sessionId: String?) async throws -> AgentPaneHandshake { throw AgentPaneHostError.daemonStopped }
    }

    /// Stands in for Chromium's page window: records the key events it was sent.
    private final class PageWindow: NSWindow {
        var sent: [String] = []
        override func sendEvent(_ event: NSEvent) { sent.append(event.characters ?? "") }
    }

    private func pane() async throws -> (AgentPaneView, FakeChromiumPage, NSWindow) {
        _ = NSApplication.shared
        let file = try #require(AgentPageTestRoot.page)
        let hosting = FakeChromiumHosting()
        let view = try #require(AgentPaneView(model: AgentPaneModel(host: NoHost()), source: .bundled(file),
                                              pageHost: true, chromium: hosting))
        let page = try #require(view.page)
        await page.chromiumStart?.value
        let chromium = try #require(hosting.made.first)
        chromium.answer = { method, _ in method == "Runtime.evaluate" ? #"{"result":{"type":"boolean","value":true}}"# : nil }
        let window = NSWindow(contentRect: view.bounds, styleMask: .titled, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        return (view, chromium, window)
    }

    private static func key(_ characters: String, modifiers: NSEvent.ModifierFlags = []) throws -> NSEvent {
        try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: ProcessInfo.processInfo.systemUptime,
                                      windowNumber: 0, context: nil, characters: characters, charactersIgnoringModifiers: characters,
                                      isARepeat: false, keyCode: 0))
    }

    private static func pageWindow() -> PageWindow {
        let window = PageWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 100), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        return window
    }

    /// The texts typed into the page's focused field through its input bridge, in order.
    private static func typed(_ chromium: FakeChromiumPage) -> [String] {
        chromium.calls.compactMap { call -> String? in
            guard call.method == "Runtime.evaluate", let expression = call.params["expression"] as? String,
                  let range = expression.range(of: "document.execCommand('insertText', false, ") else { return nil }
            let rest = expression[range.upperBound...]
            guard let end = rest.range(of: ");") else { return nil }
            return (try? JSONSerialization.jsonObject(with: Data(rest[..<end.lowerBound].utf8), options: [.fragmentsAllowed])) as? String
        }
    }

    @Test(arguments: 0..<20) func heldKeysReachTheFieldWhileTheAppIsNotActive(round: Int) async throws {
        let (view, chromium, window) = try await pane()
        let page = Self.pageWindow()
        defer { view.page?.close(); window.close(); page.close() }
        for character in ["/", "m", "o"] { view.sendHeldKey(try Self.key(character), to: page, appIsActive: false) }
        await view.heldKeyTyping?.value
        #expect(Self.typed(chromium).joined() == "/mo", "round \(round): the field got \(Self.typed(chromium)) of the held keys; Chromium dropped \(page.sent) sent to a page window that is not key")
    }

    /// The person's own path (the app active, the page window key) keeps the key event: a chord or a
    /// Return acts as typed, and the page sees each keydown.
    @Test func heldKeysStayKeyEventsWhileTheAppIsActive() async throws {
        let (view, chromium, window) = try await pane()
        let page = Self.pageWindow()
        defer { view.page?.close(); window.close(); page.close() }
        view.sendHeldKey(try Self.key("/"), to: page, appIsActive: true)
        await view.heldKeyTyping?.value
        #expect(page.sent == ["/"])
        #expect(Self.typed(chromium).isEmpty)
    }

    /// A key that types no text (Return, an arrow, a chord) is no insertion: it goes on as a key event.
    @Test func keysThatTypeNoTextStayKeyEvents() async throws {
        let (view, chromium, window) = try await pane()
        let page = Self.pageWindow()
        defer { view.page?.close(); window.close(); page.close() }
        view.sendHeldKey(try Self.key("\r"), to: page, appIsActive: false)
        view.sendHeldKey(try Self.key(String(UnicodeScalar(NSUpArrowFunctionKey)!)), to: page, appIsActive: false)
        view.sendHeldKey(try Self.key("a", modifiers: .command), to: page, appIsActive: false)
        await view.heldKeyTyping?.value
        #expect(page.sent.count == 3)
        #expect(Self.typed(chromium).isEmpty)
    }
}
