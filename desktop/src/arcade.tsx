import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ArcadeWindow } from "@/ui/ArcadeRoom";
import { TooltipProvider } from "@/ui/primitives";
import "./index.css";

// The arcade's window: no client and no store, only the games and a channel
// back to the main window. See `lib/arcadeWindow.ts`.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <ArcadeWindow />
    </TooltipProvider>
  </StrictMode>,
);
