import AppKit
@testable import CmuxNextBrowser
import CmuxNextTestSupport
import Foundation
import Testing

/// Throwaway: prints the field editor's geometry after focus for a short and a long address.
@MainActor
@Suite(.closesItsWindows)
struct OmnibarScrollDiagnosticTests {
    private func geometry(_ bar: AddressBarView) -> String {
        let field = bar.field
        guard let editor = field.currentEditor() as? NSTextView, let clip = editor.superview as? NSClipView else { return "no editor" }
        let origin = editor.convert(editor.textContainerOrigin, to: field)
        let caret = editor.firstRect(forCharacterRange: NSRange(location: 0, length: 0), actualRange: nil)
        return "clipBounds=\(clip.bounds) clipFrame=\(clip.frame) clipDoc=\(clip.documentRect) editorFrame=\(editor.frame) "
            + "inset=\(editor.textContainerInset) containerOrigin=\(editor.textContainerOrigin) originInField=\(origin) "
            + "title=\(field.cell?.titleRect(forBounds: field.bounds) ?? .zero) sel=\(editor.selectedRange()) "
            + "tk2=\(editor.textLayoutManager != nil) caretScreen=\(caret) clipClass=\(type(of: clip)) superClip=\(String(describing: clip.superview.map { type(of: $0) }))"
    }

    @Test(arguments: ["https://claude.ai/login", OmnibarLongAddressTests.longAddress])
    func printGeometry(_ address: String) throws {
        let bar = AddressBarView()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 80), styleMask: [.titled], backing: .buffered, defer: false)
        defer { window.closeAfterTest() }
        let content = try #require(window.contentView)
        content.addSubview(bar)
        NSLayoutConstraint.activate([
            bar.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 10),
            bar.widthAnchor.constraint(equalToConstant: 520),
            bar.topAnchor.constraint(equalTo: content.topAnchor, constant: 20),
        ])
        bar.update(url: try #require(URL(string: address)), security: .secure)
        content.layoutSubtreeIfNeeded()
        bar.focus()
        content.layoutSubtreeIfNeeded()
        let afterFocus = geometry(bar)
        let editor = try #require(bar.field.currentEditor() as? NSTextView)
        editor.setSelectedRange(NSRange(location: 0, length: 0))
        editor.scrollRangeToVisible(NSRange(location: 0, length: 0))
        let afterCaret = geometry(bar)
        editor.selectAll(nil)
        let afterSelectAll = geometry(bar)
        let report = "ADDR \(address.count) | FOCUS \(afterFocus) | CARET0 \(afterCaret) | SELECTALL \(afterSelectAll)"
        #expect(report == "", "diagnostic")
    }
}
