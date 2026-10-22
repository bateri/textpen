/**
 * The open files, as tabs: dragged to reorder, closed with their × or a middle click,
 * with a menu on a right click. ⌃PgUp ⌃PgDn go through them, ⌃1–⌃9 go to one, ⌘E lists
 * them by when they were last seen, ⌃W closes one.
 */

import {
  IconChevronDown,
  IconEye,
  IconEyeOff,
  IconLayoutSidebar,
  IconLock,
  IconMenu2,
  IconX,
} from "@tabler/icons-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";

import { FileIcon, basename, dirname, extension } from "../lib/files";
import { label } from "../lib/keys";
import { activate, closeOthers, copyPath, moveTab, rename, togglePreview } from "../state/actions";
import { closeAndRemember, focusSidebar, openPalette, run } from "../state/commands";
import { get, set, sidebarDocked, useStore } from "../state/store";
import type { Tab } from "../state/store";
import { openMenu } from "./Menu";
import { IconButton } from "./ui";

/** Each tab's name, with as much of its directory as tells two of the same name apart. */
function names(tabs: Tab[]): Map<string, string | null> {
  const byName = new Map<string, Tab[]>();
  for (const tab of tabs) {
    const name = basename(tab.path);
    byName.set(name, [...(byName.get(name) ?? []), tab]);
  }
  const out = new Map<string, string | null>();
  for (const group of byName.values()) {
    for (const tab of group) {
      if (group.length === 1) {
        out.set(tab.path, null);
        continue;
      }
      const parts = dirname(tab.path).split("/");
      let take = 1;
      const hint = (n: number, path: string) => dirname(path).split("/").slice(-n).join("/");
      while (
        take < parts.length &&
        group.some((o) => o !== tab && hint(take, o.path) === hint(take, tab.path))
      ) {
        take += 1;
      }
      out.set(tab.path, hint(take, tab.path) || "./");
    }
  }
  return out;
}

export function Tabs() {
  const tabs = useStore((s) => s.tabs);
  const active = useStore((s) => s.active);
  const git = useStore((s) => s.git);
  const layout = useStore((s) => s.layout);
  const docked = useStore((s) => sidebarDocked(s));
  const preview = useStore((s) => s.preview);
  const strip = useRef<HTMLDivElement>(null);
  const hints = useMemo(() => names(tabs), [tabs]);
  const [dragging, setDragging] = useState<string | null>(null);

  useEffect(() => {
    const element = strip.current?.querySelector<HTMLElement>('[data-active="true"]');
    element?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active, tabs.length]);

  const isMarkdown = active !== null && ["md", "markdown", "mdx"].includes(extension(active));
  const previewing = active !== null && preview.includes(active);

  return (
    <div className="tabs" role="tablist" aria-label="Open files">
      {!docked && (
        <IconButton
          label={layout === "narrow" ? "Files" : "Show sidebar"}
          keys="Mod-b"
          className="tabs-side"
          onClick={() =>
            layout === "narrow" ? set({ drawer: true, focus: "sidebar" }) : focusSidebar("files")
          }
        >
          {layout === "narrow" ? (
            <IconMenu2 size={16} stroke={1.6} />
          ) : (
            <IconLayoutSidebar size={16} stroke={1.6} />
          )}
        </IconButton>
      )}
      <div
        className="tabs-strip"
        ref={strip}
        onWheel={(event) => {
          if (strip.current && Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
            strip.current.scrollLeft += event.deltaY;
          }
        }}
      >
        {tabs.map((tab) => (
          <TabItem
            key={tab.path}
            tab={tab}
            hint={hints.get(tab.path) ?? null}
            active={tab.path === active}
            git={git?.changes[tab.path]}
            dragging={dragging === tab.path}
            onDragStart={() => setDragging(tab.path)}
            onDragEnd={() => setDragging(null)}
            onDropOn={() => {
              if (dragging === null || dragging === tab.path) return;
              const list = get().tabs;
              const from = list.findIndex((t) => t.path === dragging);
              const to = list.findIndex((t) => t.path === tab.path);
              const next = [...list];
              const [moved] = next.splice(from, 1);
              next.splice(to, 0, moved!);
              set({ tabs: next });
            }}
          />
        ))}
      </div>
      {isMarkdown && (
        <IconButton
          label={previewing ? "Hide preview" : "Preview"}
          keys="Shift-Mod-v"
          pressed={previewing}
          onClick={() => togglePreview()}
        >
          {previewing ? <IconEyeOff size={16} stroke={1.6} /> : <IconEye size={16} stroke={1.6} />}
        </IconButton>
      )}
      {tabs.length > 1 && (
        <IconButton label="Open files" keys="Mod-e" onClick={() => openPalette("tabs")}>
          <IconChevronDown size={16} stroke={1.6} />
        </IconButton>
      )}
    </div>
  );
}

const TabItem = memo(function TabItem({
  tab,
  hint,
  active,
  git,
  dragging,
  onDragStart,
  onDragEnd,
  onDropOn,
}: {
  tab: Tab;
  hint: string | null;
  active: boolean;
  git: string | undefined;
  dragging: boolean;
  onDragStart: () => void;
  onDragEnd: () => void;
  onDropOn: () => void;
}) {
  const dirty = tab.info?.dirty === true;
  const missing = tab.info?.missing === true && tab.info.mtime !== 0;
  const conflict = tab.info?.conflict === true;
  return (
    <div
      className="tab"
      role="tab"
      aria-selected={active}
      data-active={active}
      data-dirty={dirty}
      data-git={git}
      data-missing={missing}
      data-dragging={dragging}
      title={`${tab.path}${dirty ? " · not saved" : ""}${tab.info?.readonly ? " · read-only" : ""}`}
      draggable
      onDragStart={(event) => {
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", tab.path);
        onDragStart();
      }}
      onDragEnd={onDragEnd}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        onDropOn();
      }}
      onMouseDown={(event) => {
        event.preventDefault();
        if (event.button === 1) void closeAndRemember(tab.path);
      }}
      onClick={() => activate(tab.path)}
      onContextMenu={(event) => {
        event.preventDefault();
        openMenu({ x: event.clientX, y: event.clientY }, [
          { label: "Close", keys: "Ctrl-w", run: () => void closeAndRemember(tab.path) },
          { label: "Close Others", run: () => void closeOthers(tab.path) },
          { label: "Close All", run: () => void closeOthers(null) },
          { separator: true },
          {
            label: "Move Left",
            keys: "Shift-Ctrl-PageUp",
            run: () => {
              activate(tab.path);
              moveTab(-1);
            },
          },
          {
            label: "Move Right",
            keys: "Shift-Ctrl-PageDown",
            run: () => {
              activate(tab.path);
              moveTab(1);
            },
          },
          { separator: true },
          { label: "Rename…", keys: "F2", run: () => void rename(tab.path) },
          { label: "Copy Path", run: () => void copyPath(tab.path, false) },
          {
            label: "Reveal in Files",
            run: () => {
              activate(tab.path, false);
              run("reveal");
            },
          },
        ]);
      }}
    >
      <FileIcon path={tab.path} size={14} />
      <span className="tab-name">
        {basename(tab.path)}
        {hint && <span className="tab-hint"> {hint}</span>}
      </span>
      {tab.info?.readonly && (
        <IconLock size={12} stroke={1.8} className="tab-lock" aria-label="read-only" />
      )}
      {conflict && (
        <span className="tab-flag bad" aria-label="changed on disk">
          !
        </span>
      )}
      <button
        type="button"
        className="tab-close"
        aria-label={`Close ${basename(tab.path)} (${label("Ctrl-w")})`}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation();
          void closeAndRemember(tab.path);
        }}
      >
        <span className="tab-dot" />
        <IconX size={13} stroke={2} className="tab-x" />
      </button>
    </div>
  );
});
