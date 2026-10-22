import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

if (import.meta.env.DEV) {
  // For the harness and the browser's console: the store and the editor, to look at.
  Object.assign(window, {
    textpen: {
      store: await import("./state/store"),
      editor: (await import("./editor/controller")).editor,
    },
  });
}
