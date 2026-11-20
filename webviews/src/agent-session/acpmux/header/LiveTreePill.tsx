import { useRef, useState } from "react";
import { Icon } from "../icons/Icon";
import { Popover } from "../../../ui/Popover";
import { usePopoverTrigger } from "../../../ui/popoverTrigger";

/// The first useful live-tree slice: an opt-in header marker that makes the shared edit path and
/// the compile-lane shape visible while the live-tree service itself is still being integrated.
export function LiveTreePill({ cwd }: { cwd?: string }) {
  const [open, setOpen] = useState(false);
  const [button, setButton] = useState<HTMLButtonElement | null>(null);
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const trigger = usePopoverTrigger(open, setOpen);

  return (
    <>
      <button
        ref={(element) => {
          buttonRef.current = element;
          setButton(element);
        }}
        type="button"
        className="acpmux-header-tool acpmux-header-live-tree"
        aria-label="Live tree"
        {...trigger}
      >
        <Icon name="git.worktree" size={14} />
        <span>Live tree</span>
      </button>
      <Popover
        id={trigger["aria-controls"]}
        open={open}
        onOpenChange={(next) => {
          if (!next) setOpen(false);
        }}
        anchor={button}
        label="Live tree"
        className="acpmux-pr-status acpmux-live-tree-status"
        side="bottom"
        finalFocus={buttonRef}
      >
        <div className="acpmux-pr-title">Live tree</div>
        <div className="acpmux-pr-facts">Shared edit path</div>
        <code className="acpmux-live-tree-path">{cwd || "No working path"}</code>
        <ul className="acpmux-live-tree-lanes" aria-label="Compile lanes">
          <li>Rust</li>
          <li>Webviews</li>
          <li>Swift</li>
        </ul>
        <div className="acpmux-pr-facts">Settings › Advanced › Labs › Live Tree</div>
      </Popover>
    </>
  );
}
