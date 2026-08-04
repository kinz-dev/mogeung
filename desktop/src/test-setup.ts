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
