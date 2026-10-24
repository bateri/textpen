import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { onContext } from "./lib/page";
import { applyContext } from "./lib/theme";
import "./styles.css";

// The terminal's colours and font, now and whenever its theme changes.
onContext(applyContext);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
