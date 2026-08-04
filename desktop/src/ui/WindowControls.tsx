/**
 * The window's own title bar, folded into the top strip.
 *
 * The OS bar spent a whole row on the word "mogeung" and three buttons. That is
 * ~35px of vertical space, permanently, in a tool whose entire job is fitting a
 * board and a diff on one screen — which is why IntelliJ merges it and why this
 * does too.
 *
 * The trade is real and worth stating: turning decorations off means the window
 * manager stops providing dragging, maximise-on-double-click and the resize
 * border, so the application has to. Everything below exists because of that,
 * and if any of it is missing the window becomes one you cannot move.
 */

import { useEffect, useState } from "react";
import { Minus, Square, X, Copy } from "lucide-react";
import { isTauri } from "@/lib/tauri";
import { cn } from "@/lib/cn";

async function win() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const w = await win();
      const sync = async () => {
        const m = await w.isMaximized();
        if (!cancelled) setMaximized(m);
      };
      await sync();
      // The glyph has to follow the window, not only our own button: a
      // super-key snap or a double-click on the drag region maximises it too.
      unlisten = await w.onResized(() => void sync());
      if (cancelled) unlisten?.();
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (!isTauri()) return null;

  const Btn = ({
    onClick,
    title,
    danger,
    children,
  }: {
    onClick: () => void;
    title: string;
    danger?: boolean;
    children: React.ReactNode;
  }) => (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      className={cn(
        "grid h-6 w-8 shrink-0 place-items-center rounded-sm text-[var(--dim)] outline-none",
        "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
        "focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2",
        danger
          ? "hover:bg-[var(--red)] hover:text-[var(--on-accent)]"
          : "hover:bg-[var(--state-hover)] hover:text-[var(--text)]",
      )}
    >
      {children}
    </button>
  );

  return (
    <div className="ml-1 flex shrink-0 items-center gap-0.5">
      <Btn title="minimise" onClick={() => void win().then((w) => w.minimize())}>
        <Minus size={12} />
      </Btn>
      <Btn
        title={maximized ? "restore" : "maximise"}
        onClick={() => void win().then((w) => w.toggleMaximize())}
      >
        {maximized ? <Copy size={11} /> : <Square size={11} />}
      </Btn>
      <Btn title="close" danger onClick={() => void win().then((w) => w.close())}>
        <X size={13} />
      </Btn>
    </div>
  );
}

/**
 * A resize grip in the bottom-right corner.
 *
 * Undecorated windows lose the window manager's invisible resize border, and
 * whether GTK leaves any of it behind is not something to find out *after*
 * shipping a window nobody can resize. This costs a few pixels and guarantees
 * at least one way, whatever the compositor does.
 */
export function ResizeGrip() {
  if (!isTauri()) return null;
  return (
    <div
      title="drag to resize"
      onMouseDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        void win().then((w) => w.startResizeDragging("SouthEast" as never));
      }}
      className="absolute right-0 bottom-0 z-20 h-3 w-3 cursor-nwse-resize"
    />
  );
}
