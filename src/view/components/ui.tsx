/**
 * Small pieces the screens share: keys as the platform writes them, buttons that are
 * only an icon, highlighted matches, and a list that draws only the rows in sight.
 */

import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode, RefObject } from "react";

import { glyphs, isMac } from "../lib/keys";

/** A key: `⌃W` drawn as keycaps. */
export function Keys({ spec, quiet = false }: { spec: string; quiet?: boolean }) {
  const parts = glyphs(spec);
  return (
    <span className="keys" data-quiet={quiet} aria-label={parts.join(isMac ? " " : "+")}>
      {parts.map((part, i) => (
        <kbd key={i}>{part}</kbd>
      ))}
    </span>
  );
}

export function IconButton({
  label,
  keys,
  onClick,
  children,
  pressed,
  disabled,
  className,
}: {
  label: string;
  keys?: string;
  onClick: (event: React.MouseEvent) => void;
  children: ReactNode;
  pressed?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      className={`icon-button${className ? ` ${className}` : ""}`}
      aria-label={label}
      aria-pressed={pressed}
      data-tip={keys ? `${label}  ${glyphs(keys).join(isMac ? "" : "+")}` : label}
      disabled={disabled}
      onClick={onClick}
      onMouseDown={(e) => e.preventDefault()}
    >
      {children}
    </button>
  );
}

/** Text with the characters at `positions` marked. */
export function Highlighted({
  text,
  positions,
  offset = 0,
}: {
  text: string;
  positions: readonly number[];
  offset?: number;
}) {
  if (positions.length === 0) return <>{text}</>;
  const marked = new Set(positions.map((p) => p - offset));
  const parts: ReactNode[] = [];
  let run = "";
  let runMarked = false;
  const flush = (key: number) => {
    if (run === "") return;
    parts.push(runMarked ? <mark key={key}>{run}</mark> : <span key={key}>{run}</span>);
    run = "";
  };
  for (let i = 0; i < text.length; i += 1) {
    const isMarked = marked.has(i);
    if (isMarked !== runMarked) {
      flush(i);
      runMarked = isMarked;
    }
    run += text[i];
  }
  flush(text.length);
  return <>{parts}</>;
}

/**
 * The rows of a long list that are in sight (and a few around them), for a scroller of
 * rows of one height. `reveal` scrolls a row into sight.
 */
export function useWindow(count: number, rowHeight: number) {
  const ref = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ top: 0, height: 600 });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => setView({ top: element.scrollTop, height: element.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    element.addEventListener("scroll", measure, { passive: true });
    return () => {
      observer.disconnect();
      element.removeEventListener("scroll", measure);
    };
  }, []);
  const overscan = 8;
  const start = Math.max(0, Math.floor(view.top / rowHeight) - overscan);
  const end = Math.min(count, Math.ceil((view.top + view.height) / rowHeight) + overscan);
  const reveal = useCallback(
    (index: number) => {
      const element = ref.current;
      if (!element || index < 0) return;
      const top = index * rowHeight;
      const bottom = top + rowHeight;
      const height = element.clientHeight;
      // A row a step out of sight (the keys moving through the list) comes in at the edge;
      // one far away (a file opened elsewhere) comes to the middle, with what is around it.
      const far =
        top < element.scrollTop - rowHeight || bottom > element.scrollTop + height + rowHeight;
      if (far) element.scrollTop = Math.max(0, top - (height - rowHeight) / 2);
      else if (top < element.scrollTop) element.scrollTop = top;
      else if (bottom > element.scrollTop + height) element.scrollTop = bottom - height;
    },
    [rowHeight],
  );
  return { ref: ref as RefObject<HTMLDivElement>, start, end, total: count * rowHeight, reveal };
}

/** Moves within a list of `count` rows by a key, if it is a moving key: the new index. */
export function moveBy(
  event: { key: string; ctrlKey: boolean; metaKey: boolean },
  index: number,
  count: number,
  page = 10,
  letters = true,
): number | undefined {
  if (count === 0) return undefined;
  const key = event.key;
  if (key === "ArrowDown" || (letters && key === "j") || (event.ctrlKey && key === "n")) {
    return Math.min(count - 1, index + 1);
  }
  if (key === "ArrowUp" || (letters && key === "k") || (event.ctrlKey && key === "p")) {
    return Math.max(0, index < 0 ? 0 : index - 1);
  }
  if (key === "Home" || (letters && key === "g")) return 0;
  if (key === "End" || (letters && key === "G")) return count - 1;
  if (key === "PageDown") return Math.min(count - 1, index + page);
  if (key === "PageUp") return Math.max(0, index - page);
  return undefined;
}
