/**
 * The sidebar: the files, or the search in them. Beside the editor in a pane wide enough
 * for both (its edge dragged, or moved with ← → when it has the keyboard), over it in a
 * narrow one.
 */

import {
  IconFilePlus,
  IconFiles,
  IconFolderPlus,
  IconLayoutSidebarLeftCollapse,
  IconListTree,
  IconSearch,
} from "@tabler/icons-react";
import { useRef } from "react";

import { label } from "../lib/keys";
import { changeSettings, create, listDir } from "../state/actions";
import { focusEditor, focusSidebar } from "../state/commands";
import { get, set, useStore } from "../state/store";
import { FileTree } from "./FileTree";
import { ProjectSearch } from "./ProjectSearch";
import { IconButton } from "./ui";

export function Sidebar({ drawer }: { drawer: boolean }) {
  const panel = useStore((s) => s.sidebar);
  const rootName = useStore((s) => s.boot?.rootName ?? "");
  const git = useStore((s) => s.git);
  const changes = git ? Object.keys(git.changes).length : 0;

  return (
    <aside className="sidebar" data-drawer={drawer} aria-label="Sidebar">
      <div className="sidebar-head">
        <div className="segmented" role="tablist" aria-label="Sidebar">
          <button
            type="button"
            role="tab"
            aria-selected={panel === "files"}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => focusSidebar("files")}
            data-tip={`Files  ${label("Shift-Mod-e")}`}
          >
            <IconFiles size={14} stroke={1.7} />
            <span>Files</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={panel === "search"}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => focusSidebar("search")}
            data-tip={`Search in files  ${label("Shift-Mod-f")}`}
          >
            <IconSearch size={14} stroke={1.7} />
            <span>Search</span>
          </button>
        </div>
        <span className="grow" />
        {panel === "files" && (
          <>
            <IconButton label="New file" keys="Alt-Mod-n" onClick={() => void create("file")}>
              <IconFilePlus size={15} stroke={1.6} />
            </IconButton>
            <IconButton label="New folder" onClick={() => void create("dir")}>
              <IconFolderPlus size={15} stroke={1.6} />
            </IconButton>
            <IconButton
              label="Collapse folders"
              onClick={() => {
                set({ expanded: [] });
                void listDir("");
              }}
            >
              <IconListTree size={15} stroke={1.6} />
            </IconButton>
          </>
        )}
        <IconButton
          label={drawer ? "Close" : "Hide sidebar"}
          keys="Mod-b"
          onClick={() => {
            if (drawer) set({ drawer: false });
            else changeSettings({ sidebar: false });
            focusEditor();
          }}
        >
          <IconLayoutSidebarLeftCollapse size={15} stroke={1.6} />
        </IconButton>
      </div>
      {panel === "files" && (
        <div className="project-line" title={get().boot?.root}>
          <span className="project-name ellipsis">{rootName}</span>
          {git && (
            <span className="faint ellipsis">
              {git.branch}
              {changes > 0 && ` · ${changes} changed`}
            </span>
          )}
        </div>
      )}
      {panel === "files" ? <FileTree /> : <ProjectSearch />}
    </aside>
  );
}

/** The edge between the sidebar and the editor, dragged or moved with the arrows. */
export function Resizer() {
  const width = useStore((s) => s.settings.sidebarWidth);
  const start = useRef<{ x: number; width: number } | null>(null);
  const clamp = (value: number) =>
    Math.min(Math.max(160, Math.round(value)), Math.min(520, window.innerWidth - 280));
  return (
    <div
      className="resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      aria-valuenow={width}
      aria-valuemin={160}
      aria-valuemax={520}
      tabIndex={-1}
      onPointerDown={(event) => {
        event.preventDefault();
        (event.target as HTMLElement).setPointerCapture(event.pointerId);
        start.current = { x: event.clientX, width: get().settings.sidebarWidth };
        document.body.dataset.resizing = "true";
      }}
      onPointerMove={(event) => {
        if (!start.current) return;
        const next = clamp(start.current.width + event.clientX - start.current.x);
        set((s) => ({ settings: { ...s.settings, sidebarWidth: next } }));
      }}
      onPointerUp={() => {
        if (!start.current) return;
        start.current = null;
        delete document.body.dataset.resizing;
        changeSettings({ sidebarWidth: get().settings.sidebarWidth });
      }}
      onDoubleClick={() => changeSettings({ sidebarWidth: 240 })}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
          event.preventDefault();
          changeSettings({ sidebarWidth: clamp(width + (event.key === "ArrowLeft" ? -16 : 16)) });
        }
      }}
    />
  );
}
