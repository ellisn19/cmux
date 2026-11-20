import CmuxNextSettings
import Testing

/// `labs.personalOrchestrationMonitor`: the read-only monitor is an explicit Labs experiment.
struct PersonalOrchestrationMonitorSettingTests {
    private static func snapshot(_ value: JSONValue?) -> CmuxConfigSnapshot {
        var root: [String: JSONValue] = [:]
        if let value { root["labs"] = .object(["personalOrchestrationMonitor": value]) }
        return CmuxConfigSnapshot.parse(.object(root), validDensities: [], validMetrics: [])
    }

    @Test func offByDefault() {
        let snapshot = Self.snapshot(nil)
        #expect(!snapshot.personalOrchestrationMonitor)
        #expect(snapshot.diagnostics.isEmpty)
    }

    @Test func thePersonCanTurnItOn() {
        #expect(Self.snapshot(.bool(true)).personalOrchestrationMonitor)
        #expect(!Self.snapshot(.bool(false)).personalOrchestrationMonitor)
    }

    @Test func invalidValuesStayOffAndDiagnose() {
        let snapshot = Self.snapshot(.string("yes"))
        #expect(!snapshot.personalOrchestrationMonitor)
        #expect(snapshot.diagnostics.contains { $0.path == "labs.personalOrchestrationMonitor" })
    }

    @Test func settingsListsTheToggleInLabs() throws {
        let descriptor = try #require(SettingsSchema.descriptor(for: ["labs", "personalOrchestrationMonitor"]))
        #expect(descriptor.defaultValue == .bool(false))
        #expect(descriptor.section == .advanced)
        #expect(descriptor.kind == .toggle)
        #expect(SettingsSchema.agentRefusedKeys["labs.personalOrchestrationMonitor"] == .userOnly)
    }
}
