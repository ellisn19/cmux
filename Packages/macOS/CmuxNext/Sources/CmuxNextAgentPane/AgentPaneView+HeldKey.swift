public import AppKit

extension AgentPaneView {
    /// Sends `event`, a key the New Tab key hold kept, on to `pageWindow`, the window this pane's
    /// Chromium page shows in.
    ///
    /// - Parameters:
    ///   - event: The held key, already retargeted to the page's window.
    ///   - pageWindow: The Chromium page window (a child of the pane's window).
    ///   - appIsActive: Whether cmux is the active app (injected for tests).
    public func sendHeldKey(_ event: NSEvent, to pageWindow: NSWindow, appIsActive: Bool = NSApp.isActive) {
        pageWindow.sendEvent(event)
    }

    /// The agent pane in `window` whose Chromium page shows in `pageWindow` (a child window of
    /// `window`), found by the page window's center; nil when no pane's page covers it.
    ///
    /// - Parameters:
    ///   - pageWindow: A Chromium page window.
    ///   - window: The cmux window it is a child of.
    /// - Returns: The pane, or nil.
    public static func showing(chromiumPageWindow pageWindow: NSWindow, in window: NSWindow) -> AgentPaneView? {
        let center = CGPoint(x: pageWindow.frame.midX, y: pageWindow.frame.midY)
        var stack: [NSView] = window.contentView.map { [$0] } ?? []
        while let view = stack.popLast() {
            if let pane = view as? AgentPaneView, let content = pane.chromiumPage?.contentView, let host = content.window,
               host.convertToScreen(content.convert(content.bounds, to: nil)).contains(center) { return pane }
            stack.append(contentsOf: view.subviews)
        }
        return nil
    }
}
