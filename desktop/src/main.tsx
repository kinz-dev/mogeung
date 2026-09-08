import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./lib/monaco-setup";
import App from "./App";
import { PopoutBoot } from "./PopoutApp";
import { readPopout } from "./lib/popout";
import { startClient } from "./store";
import "./index.css";

// One socket for the process, started before the first render so the snapshot
// is usually already in flight by the time anything asks for it. A popout is a
// second process-worth of client in its own window (`R-B55`, ADR-0037), so it
// opens its own — which is the cost of every window being an ordinary client.
startClient();

// Which of the two things this window is. Read from the query string rather
// than from the store, because it decides what gets rendered at all and has to
// be answerable before anything else exists.
const popout = readPopout();

createRoot(document.getElementById("root")!).render(
  <StrictMode>{popout ? <PopoutBoot popout={popout} /> : <App />}</StrictMode>,
);
