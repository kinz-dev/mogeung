import "@testing-library/jest-dom/vitest";

// jsdom has neither of these, and both are load-bearing for a layout that
// measures itself: dockview sizes its groups from ResizeObserver, and the
// virtualised lists ask for element rects.
class RO {
  observe() {}
  unobserve() {}
  disconnect() {}
}
(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = RO;

if (!window.matchMedia) {
  window.matchMedia = ((q: string) => ({
    matches: false,
    media: q,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}

if (!Element.prototype.scrollIntoView) Element.prototype.scrollIntoView = () => {};

/**
 * `localStorage`, on a Node that ships its own. `R-J19`.
 *
 * Node 25 defines a global `localStorage` of its own, and it is inert without
 * `--localstorage-file` — the process prints *"`--localstorage-file` was
 * provided without a valid path"* once and then hands out an object with no
 * `getItem` on it. vitest's jsdom environment copies the jsdom window's
 * properties onto `globalThis` **without overwriting what is already there**,
 * so Node's stub wins and jsdom's real `Storage` becomes unreachable — even
 * through `window.localStorage`, because in this environment `window` *is*
 * `globalThis`.
 *
 * The symptom is 26 of 44 test files failing at import time with
 * `localStorage.getItem is not a function`, in files that never mention
 * storage: the store reads a saved daemon URL as it is constructed, so
 * importing anything that imports the store is enough. Nothing in the product
 * is wrong and every one of those failures is the toolchain.
 *
 * A `Map` is the whole implementation because that is all the real one is here
 * — the browser this ships in has a genuine `localStorage`, and what the tests
 * need is somewhere that remembers a string until `clear()`.
 */
if (typeof localStorage !== "object" || typeof localStorage?.getItem !== "function") {
  const cells = new Map<string, string>();
  const storage: Storage = {
    get length() {
      return cells.size;
    },
    key: (i) => [...cells.keys()][i] ?? null,
    getItem: (k) => cells.get(String(k)) ?? null,
    setItem: (k, v) => void cells.set(String(k), String(v)),
    removeItem: (k) => void cells.delete(String(k)),
    clear: () => cells.clear(),
  };
  Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true, writable: true });
}
