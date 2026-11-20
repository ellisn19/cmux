// l10n-allow-file: gallery-only sample labels, not shipped UI.
import { componentEntry } from "../../../gallery/format";
import type { Play } from "../../../gallery/play";
import { LiveTreePill } from "./LiveTreePill";

const open: Play = async (ctx) => {
  await ctx.click({ selector: ".acpmux-header-live-tree" });
  await ctx.waitFor(() => ctx.document.querySelector('[role="dialog"]'));
};

function LiveTreeGallery({ cwd }: { cwd?: string }) {
  return (
    <div className="acpmux-shell" style={{ minHeight: 180 }}>
      <header className="acpmux-header">
        <div className="acpmux-header-status-slot">Agent session</div>
        <div className="acpmux-handoff-header-tools acpmux-header-tools">
          <LiveTreePill cwd={cwd} />
        </div>
      </header>
      <main style={{ padding: 20, color: "var(--agent-muted)" }}>
        The default pane is unchanged until Labs enables this marker.
      </main>
    </div>
  );
}

export default componentEntry<{ cwd?: string }>({
  id: "agent-pane.live-tree-pill",
  title: "Live tree marker",
  area: "Agent pane",
  pane: true,
  height: 220,
  widths: { narrow: 360, normal: 520, wide: 760 },
  anchors: [{ selector: ".acpmux-header-live-tree" }],
  covers: ["agent-session/acpmux/header/LiveTreePill.tsx#LiveTreePill"],
  styles: () => Promise.all([import("../styles.css"), import("./header.css")]),
  load: async () => LiveTreeGallery,
  variants: {
    "local-path": {
      props: { cwd: "/work/livetree/tree" },
      play: open,
      note: "The first live-tree slice shows the shared path and Rust, Webviews and Swift lanes.",
    },
    "long-path": {
      props: { cwd: "/Users/example/Projects/a-very-long-project-name/feature/live-tree" },
      play: open,
      note: "Long working paths wrap inside the popover without changing the header geometry.",
    },
    "missing-path": {
      props: {},
      play: open,
      note: "A missing chat working path gets a readable fallback while the experiment remains visible.",
    },
  },
});
