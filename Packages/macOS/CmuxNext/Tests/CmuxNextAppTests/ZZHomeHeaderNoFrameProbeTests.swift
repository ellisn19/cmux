import AppKit
import CmuxNextDesign
import CmuxNextDesignTokens
@testable import CmuxNextHome
import CmuxNextTestSupport
import Foundation
import Testing

/// PROBE (not for landing): HomeHeaderStaysPutTests with its frame wait cut short, as a stray CFRunLoopStop cuts it.
@MainActor @Suite(.serialized, .closesItsWindows) struct ZZHomeHeaderNoFrameProbeTests {
    @Test(arguments: [DensityPreset.compact, .spacious])
    func probe(density: DensityPreset) throws {
        _ = NSApplication.shared
        let design = DesignSettings.shared
        let saved = (design.densityTokens, design.titlebar, design.tabBarPosition)
        defer { (design.densityTokens, design.titlebar, design.tabBarPosition) = saved }
        design.densityTokens = density.tokens
        design.titlebar = .minimal
        design.tabBarPosition = .top
        let suite = "cmux.tests.probe.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(testSuiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let home = HomeSidebarView(frame: NSRect(x: 0, y: 0, width: 320, height: 800))
        let split = HomeSidebarSplitView(sidebar: home, content: NSView(frame: NSRect(x: 0, y: 0, width: 880, height: 800)),
                                         widths: HomeSidebarWidth(defaults: defaults))
        split.windowKey = { "w" }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.titlebarAppearsTransparent = true
        defer { window.close() }
        window.contentView = split
        split.setWidth(320)
        TrafficLightsLine.apply(in: window)
        settle(home, split)
        func snap(_ tag: String) {
            let lights = WindowTitlebar.trafficLightsFrame(in: window) ?? .zero
            let close = window.standardWindowButton(.closeButton)
            let titlebar = close?.superview
            let field = home.list.searchField
            print("PROBE|\(density.rawValue)|\(tag)|lights=\(lights)|row=\(String(describing: home.list.headerView.homeLightsRow(band: HomeSidebarHeaderLayout.band)))|filter=\(home.header.filter)|listTop=\(home.header.listTop)|search=\(field.convert(field.bounds, to: nil))|closeFrame=\(String(describing: close?.frame))|bounds=\(String(describing: titlebar?.bounds.origin))|needsLayout=\(String(describing: titlebar?.superview?.needsLayout))")
        }
        snap("rest")
        for (mode, wait) in [("no frame", { () in }), ("one frame", { TrafficLightsStayPutTests.nextFrame() })] {
            window.title = "cmux perf lead \(mode)"
            snap("\(mode): title set")
            wait()
            snap("\(mode): after wait")
            settle(home, split)
            snap("\(mode): after settle")
            TrafficLightsStayPutTests.nextFrame()
            settle(home, split)
            snap("\(mode): after a later frame")
        }
    }

    private func settle(_ home: HomeSidebarView, _ split: NSView) {
        home.updateWindowControlsAvoidance()
        split.needsLayout = true
        split.layoutSubtreeIfNeeded()
        home.list.headerView.needsLayout = true
        home.list.headerView.layoutSubtreeIfNeeded()
    }
}
