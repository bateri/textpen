/**
 * A Markdown file as it reads: beside its text in a wide pane, in its place in a narrow
 * one. Raw HTML in the file is shown as text, not run; links open in the person's
 * browser through the terminal, and images, which the view cannot reach, are named.
 */

import { Marked } from "marked";
import { useDeferredValue, useEffect, useMemo, useRef } from "react";

import { editor } from "../editor/controller";
import { openLink } from "../lib/page";
import { useStore } from "../state/store";

const escape = (text: string) =>
  text.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!,
  );

const marked = new Marked({
  gfm: true,
  breaks: false,
  renderer: {
    html({ text }) {
      return `<code class="md-html">${escape(text)}</code>`;
    },
    image({ text, href }) {
      return `<span class="md-image" title="${escape(href)}">▣ ${escape(text || href)}</span>`;
    },
  },
});

export function Preview({ path }: { path: string }) {
  // The cursor changes with every edit: the preview follows the text through it.
  const cursor = useStore((s) => s.cursor);
  const deferred = useDeferredValue(cursor);
  const ref = useRef<HTMLDivElement>(null);
  const html = useMemo(() => {
    void deferred;
    const text = editor.text(path) ?? "";
    try {
      return marked.parse(text, { async: false });
    } catch {
      return `<pre>${escape(text)}</pre>`;
    }
  }, [path, deferred]);

  // Roughly where the cursor is in the text, the preview is too.
  useEffect(() => {
    const element = ref.current;
    if (!element || deferred.lines <= 1) return;
    const ratio = (deferred.line - 1) / (deferred.lines - 1);
    element.scrollTop = ratio * (element.scrollHeight - element.clientHeight);
  }, [deferred.line, deferred.lines]);

  return (
    <div
      className="preview"
      ref={ref}
      tabIndex={0}
      aria-label="Preview"
      onClick={(event) => {
        const anchor = (event.target as HTMLElement).closest("a");
        if (!anchor) return;
        event.preventDefault();
        const href = anchor.getAttribute("href") ?? "";
        if (/^https?:/i.test(href)) void openLink(href);
      }}
    >
      <article className="markdown" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}
