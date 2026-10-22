/**
 * Where the file is: the editor, with a strip above it when something needs a word (a
 * draft from an earlier run, a file changed or removed on disk), a preview beside a
 * Markdown file, a page for a file that is not text, and a start page with no file.
 */

import {
  IconAlertTriangle,
  IconBinary,
  IconFileAlert,
  IconHistory,
  IconLock,
  IconTrash,
} from "@tabler/icons-react";
import { useEffect, useLayoutEffect, useRef } from "react";

import { editor } from "../editor/controller";
import { ago, basename, extension, formatBytes } from "../lib/files";
import { closeTab, draft, keepMine, revert } from "../state/actions";
import { useStore } from "../state/store";
import type { Tab } from "../state/store";
import { Preview } from "./Preview";
import { Welcome } from "./Welcome";

export function EditorArea() {
  const active = useStore((s) => s.active);
  const tab = useStore((s) => s.tabs.find((t) => t.path === s.active));
  const previewing = useStore((s) => s.active !== null && s.preview.includes(s.active));
  const layout = useStore((s) => s.layout);
  const fontSize = useStore((s) => s.settings.fontSize);
  const host = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (host.current) editor.attach(host.current);
  }, []);

  useEffect(() => {
    editor.view?.requestMeasure();
  }, [fontSize, layout, previewing]);

  const ready = tab?.status === "ready";
  const markdown = active !== null && ["md", "markdown", "mdx"].includes(extension(active));
  const split = ready && markdown && previewing && layout === "wide";
  const only = ready && markdown && previewing && layout !== "wide";

  return (
    <section className="editor-area" aria-label="Editor">
      {tab && ready && <Banner tab={tab} />}
      <div className="editor-row" data-split={split}>
        <div
          className="editor-host"
          ref={host}
          data-hidden={!ready || only}
          style={{ "--editor-font-size": `${fontSize}px` } as React.CSSProperties}
        />
        {(split || only) && active && <Preview path={active} />}
        {!tab && <Welcome />}
        {tab && !ready && <NotText tab={tab} />}
      </div>
    </section>
  );
}

function Banner({ tab }: { tab: Tab }) {
  const info = tab.info;
  if (!info) return null;
  const name = basename(tab.path);
  if (info.draft) {
    return (
      <div className="banner" data-tone="info" role="status">
        <IconHistory size={15} stroke={1.7} />
        <span className="banner-text">
          Unsaved changes to <b>{name}</b> were kept when textpen last ended ({ago(info.draft.at)}).
        </span>
        <button
          type="button"
          className="text-button primary"
          onClick={() => void draft(tab.path, true)}
        >
          Restore
        </button>
        <button type="button" className="text-button" onClick={() => void draft(tab.path, false)}>
          Discard
        </button>
      </div>
    );
  }
  if (info.conflict) {
    return (
      <div className="banner" data-tone="warn" role="alert">
        <IconAlertTriangle size={15} stroke={1.7} />
        <span className="banner-text">
          <b>{name}</b> changed on disk while it had changes here.
        </span>
        <button type="button" className="text-button" onClick={() => void revert(tab.path)}>
          Reload from Disk
        </button>
        <button
          type="button"
          className="text-button primary"
          onClick={() => void keepMine(tab.path)}
        >
          Keep Mine
        </button>
      </div>
    );
  }
  if (info.missing && info.mtime !== 0) {
    return (
      <div className="banner" data-tone="bad" role="alert">
        <IconTrash size={15} stroke={1.7} />
        <span className="banner-text">
          <b>{name}</b> was deleted on disk. Saving writes it again.
        </span>
        <button type="button" className="text-button" onClick={() => void closeTab(tab.path)}>
          Close
        </button>
      </div>
    );
  }
  if (info.readonly) {
    return (
      <div className="banner" data-tone="quiet" role="status">
        <IconLock size={14} stroke={1.7} />
        <span className="banner-text">Read-only: textpen cannot write to {name}.</span>
      </div>
    );
  }
  return null;
}

function NotText({ tab }: { tab: Tab }) {
  if (tab.status === "loading") {
    return (
      <div className="placeholder" aria-busy>
        <span className="spinner" />
      </div>
    );
  }
  const reason = tab.unopenable;
  const Icon = reason?.reason === "binary" ? IconBinary : IconFileAlert;
  return (
    <div className="placeholder">
      <Icon size={36} stroke={1.2} className="placeholder-icon" />
      <div className="placeholder-title">{basename(tab.path)}</div>
      <div className="placeholder-text">
        {reason ? reason.message : (tab.error ?? "This file could not be opened.")}
        {reason && reason.size > 0 && <span className="faint"> · {formatBytes(reason.size)}</span>}
      </div>
      <button type="button" className="text-button" onClick={() => void closeTab(tab.path)}>
        Close Tab
      </button>
    </div>
  );
}
