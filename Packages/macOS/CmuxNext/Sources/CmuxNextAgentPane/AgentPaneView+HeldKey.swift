public import AppKit

extension AgentPaneView {
    /// Sends `event`, a key the New Tab key hold kept, on to `pageWindow`, the window this pane's
    /// Chromium page shows in.
    ///
    /// While the app is not active no window is key, and Chromium drops a key event sent to a page
    /// window that is not key (the document has no focus), although the field already has the caret:
    /// a script's or a background opening lost its first keys, a `/` for the slash menu included. A key
    /// that types text then goes into the focused field through the page's input bridge
    /// (``PageInputReadiness/insert(_:expectedEpoch:)``), after the text of the keys before it
    /// (``heldKeyTyping``). Any other key, and every key while the app is active, stays a key event.
    ///
    /// - Parameters:
    ///   - event: The held key, already retargeted to the page's window.
    ///   - pageWindow: The Chromium page window (a child of the pane's window).
    ///   - appIsActive: Whether cmux is the active app (injected for tests).
    public func sendHeldKey(_ event: NSEvent, to pageWindow: NSWindow, appIsActive: Bool = NSApp.isActive) {
        guard !appIsActive, chromiumPage != nil, let text = typedText(of: event) else { return pageWindow.sendEvent(event) }
        let before = heldKeyTyping
        heldKeyTyping = Task { [inputReadiness] in
            await before?.value
            _ = await inputReadiness.insert(text)
        }
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

/// The text `event` types: a key down with no Command, Control or Function modifier whose characters
/// are all printable (no Return, Tab, Escape, Delete or arrow); nil for any other key.
private func typedText(of event: NSEvent) -> String? {
    guard event.type == .keyDown, event.modifierFlags.isDisjoint(with: [.command, .control, .function]),
          let text = event.characters, !text.isEmpty else { return nil }
    let printable = text.unicodeScalars.allSatisfy { scalar in
        !CharacterSet.controlCharacters.contains(scalar) && !(0xF700...0xF8FF).contains(scalar.value)
    }
    return printable ? text : nil
}
