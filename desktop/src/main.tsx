import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./lib/monaco-setup";
import App from "./App";
import { startClient } from "./store";
import "./index.css";

// One socket for the process, started before the first render so the snapshot
// is usually already in flight by the time anything asks for it.
//
// **One root, and one client, even with panes in other windows.** A popped-out
// pane is this dockview drawing into a second document (`R-B55`, ADR-0037's
// 2026-09-08 amendment) rather than a second client — which is what lets a pane
// be dragged from one window to the other.
startClient();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
