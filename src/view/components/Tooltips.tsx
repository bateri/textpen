/**
 * Tooltips for whatever says `data-tip`: shown after the pointer rests on it, or at once
 * when the keyboard reaches it, and placed to stay inside the pane.
 */

import { useEffect, useLayoutEffect, useRef, useState } from "react";

interface Tip {
  text: string;
  rect: DOMRect;
}

export function Tooltips() {
  const [tip, setTip] = useState<Tip | null>(null);
  const [place, setPlace] = useState<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let warm = false;
    const show = (target: HTMLElement, delay: number) => {
      clearTimeout(timer);
      const text = target.dataset.tip;
      if (!text) return;
      timer = setTimeout(() => {
        warm = true;
        setTip({ text, rect: target.getBoundingClientRect() });
      }, delay);
    };
    const hide = () => {
      clearTimeout(timer);
      setTip(null);
      setTimeout(() => (warm = false), 400);
    };
    const over = (event: PointerEvent) => {
      const target = (event.target as HTMLElement).closest<HTMLElement>("[data-tip]");
      if (target) show(target, warm ? 60 : 550);
      else hide();
    };
    const focus = (event: FocusEvent) => {
      const target = (event.target as HTMLElement).closest?.<HTMLElement>("[data-tip]");
      if (target?.matches(":focus-visible")) show(target, 0);
    };
    window.addEventListener("pointerover", over);
    window.addEventListener("pointerdown", hide, true);
    window.addEventListener("keydown", hide, true);
    window.addEventListener("focusin", focus);
    window.addEventListener("focusout", hide);
    window.addEventListener("scroll", hide, true);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("pointerover", over);
      window.removeEventListener("pointerdown", hide, true);
      window.removeEventListener("keydown", hide, true);
      window.removeEventListener("focusin", focus);
      window.removeEventListener("focusout", hide);
      window.removeEventListener("scroll", hide, true);
    };
  }, []);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!tip || !element) {
      setPlace(null);
      return;
    }
    const box = element.getBoundingClientRect();
    const below = tip.rect.bottom + 6 + box.height < window.innerHeight;
    const x = Math.max(
      6,
      Math.min(
        tip.rect.left + tip.rect.width / 2 - box.width / 2,
        window.innerWidth - box.width - 6,
      ),
    );
    const y = below ? tip.rect.bottom + 6 : tip.rect.top - box.height - 6;
    setPlace({ x, y });
  }, [tip]);

  if (!tip) return null;
  return (
    <div
      ref={ref}
      className="tooltip"
      role="tooltip"
      style={
        place ? { left: place.x, top: place.y } : { left: -999, top: -999, visibility: "hidden" }
      }
    >
      {tip.text}
    </div>
  );
}
