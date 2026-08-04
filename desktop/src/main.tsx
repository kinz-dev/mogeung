import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./lib/monaco-setup";
import App from "./App";
import { startClient } from "./store";
import "./index.css";

// One socket for the process, started before the first render so the snapshot
// is usually already in flight by the time anything asks for it.
startClient();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
