/**
 * A menu at the pointer or at a row: what a right-click, `.` or ⇧F10 offers. ↑ ↓ choose,
 * ⏎ runs, Esc closes, and the letter a row shows runs it too.
 */

import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { matches } from "../lib/keys";
import { set, useStore } from "../state/store";
import type { MenuItem } from "../state/store";
import { regainFocus } from "./Palette";
import { Keys } from "./ui";

export function openMenu(at: { x: number; y: number }, items: MenuItem[]): void {
  set({ overlay: { kind: "menu", at, items } });
}

export function MenuLayer() {
  const overlay = useStore((s) => s.overlay);
  if (overlay?.kind !== "menu") return null;
  return <Menu at={overlay.at} items={overlay.items} />;
}

function Menu({ at, items }: { at: { x: number; y: number }; items: MenuItem[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const runnable = items
    .map((item, index) => ({ item, index }))
    .filter(
      (x): x is { item: Extract<MenuItem, { label: string }>; index: number } =>
        "label" in x.item && !x.item.disabled,
    );
  const [chosen, setChosen] = useState(0);
  const [place, setPlace] = useState(at);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    element.focus();
    const rect = element.getBoundingClientRect();
    setPlace({
      x: Math.max(6, Math.min(at.x, window.innerWidth - rect.width - 6)),
      y: Math.max(6, Math.min(at.y, window.innerHeight - rect.height - 6)),
    });
  }, [at]);

  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) set({ overlay: null });
    };
    window.addEventListener("mousedown", close, true);
    return () => window.removeEventListener("mousedown", close, true);
  }, []);

  const runItem = (item: { run: () => void }) => {
    set({ overlay: null });
    item.run();
    regainFocus();
  };

  return (
    <div
      ref={ref}
      className="menu pop"
      role="menu"
      tabIndex={-1}
      style={{ left: place.x, top: place.y }}
      onKeyDown={(event) => {
        event.stopPropagation();
        const native = event.nativeEvent;
        if (event.key === "Escape") {
          event.preventDefault();
          set({ overlay: null });
        } else if (event.key === "ArrowDown" || (event.ctrlKey && event.key === "n")) {
          event.preventDefault();
          setChosen((chosen + 1) % runnable.length);
        } else if (event.key === "ArrowUp" || (event.ctrlKey && event.key === "p")) {
          event.preventDefault();
          setChosen((chosen - 1 + runnable.length) % runnable.length);
        } else if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          const item = runnable[chosen]?.item;
          if (item) runItem(item);
        } else {
          const hit = runnable.find(({ item }) => item.keys && matches(native, item.keys));
          if (hit) {
            event.preventDefault();
            runItem(hit.item);
          }
        }
      }}
    >
      {items.map((item, index) =>
        "separator" in item ? (
          <div key={index} className="menu-separator" role="separator" />
        ) : (
          <button
            key={index}
            type="button"
            role="menuitem"
            className="menu-item"
            data-chosen={runnable[chosen]?.index === index}
            data-danger={item.danger}
            disabled={item.disabled}
            onMouseEnter={() => {
              const at = runnable.findIndex((r) => r.index === index);
              if (at >= 0) setChosen(at);
            }}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => runItem(item)}
          >
            <span>{item.label}</span>
            {item.keys && <Keys spec={item.keys} quiet />}
          </button>
        ),
      )}
    </div>
  );
}
