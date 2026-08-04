/**
 * Global search, as an overlay. `R-F13`.
 *
 * The same search as the rail's — literally the same component and the same
 * store state, so a query typed here is still there when you open the rail, and
 * results found in one are not re-run by the other.
 *
 * Two ways in because they are two jobs, which is the argument the palette and
 * the rail already settled between them: this is **ask and go**, over the whole
 * window and out of the way afterwards; the rail is **keep it open**, beside the
 * thing you are reading. Neither is the other with different padding.
 */

import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { SearchTool } from "@/ui/tools/SearchTool";

export function SearchOverlay() {
  const open = useStore((s) => s.searchOpen);
  if (!open) return null;

  const close = () => useStore.setState({ searchOpen: false });

  return (
    <Dialog
      title="Search"
      subtitle="this conversation · this session's files · every session"
      onClose={close}
      wide
    >
      {/* Taller than a rail, because the whole reason to overlay it is the
          room: a results list you can see twenty rows of, and a preview big
          enough to decide from without opening anything. */}
      <div className="flex h-[62vh] min-h-0 flex-col">
        <SearchTool onOpened={close} />
      </div>
    </Dialog>
  );
}
