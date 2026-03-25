/**
 * Reusable horizontal split-pane component with draggable divider.
 *
 * Renders two child panels side by side separated by a draggable divider.
 * The split ratio is persisted to localStorage via the storageKey prop.
 * Uses setPointerCapture for smooth dragging even outside component bounds.
 * Pixel-perfect port of the Svelte SplitPane.
 */

import { Component, JSX, createSignal, onMount } from "solid-js";
import "./SplitPane.css";

interface SplitPaneProps {
  /** localStorage key suffix for persisting the split ratio. */
  storageKey: string;
  /** Default left-panel ratio (0.0-1.0). */
  defaultRatio?: number;
  /** Minimum left panel width in pixels. */
  minLeftPx?: number;
  /** Minimum right panel width in pixels. */
  minRightPx?: number;
  /** Left panel content. */
  left: JSX.Element;
  /** Right panel content. */
  right: JSX.Element;
}

/** Reads a persisted split ratio from localStorage. */
function getSplitRatio(key: string, defaultVal: number): number {
  try {
    const stored = localStorage.getItem(`split-ratio-${key}`);
    if (stored !== null) {
      const parsed = parseFloat(stored);
      if (!isNaN(parsed) && parsed > 0 && parsed < 1) return parsed;
    }
  } catch {
    // localStorage may be unavailable
  }
  return defaultVal;
}

/** Persists a split ratio to localStorage. */
function setSplitRatio(key: string, ratio: number): void {
  try {
    localStorage.setItem(`split-ratio-${key}`, String(ratio));
  } catch {
    // localStorage may be unavailable
  }
}

const SplitPane: Component<SplitPaneProps> = (props) => {
  // L-65: SplitPane layout constraints are read once at initialization. These values are
  // treated as static configuration; changes to minLeftPx, minRightPx, or defaultRatio
  // after mount will not take effect. All current callers pass compile-time constants.
  const defaultRatio = props.defaultRatio ?? 0.5;
  const minLeftPx = props.minLeftPx ?? 300;
  const minRightPx = props.minRightPx ?? 250;

  let containerEl: HTMLDivElement | undefined;
  const [ratio, setRatio] = createSignal(0.5);
  const [dragging, setDragging] = createSignal(false);

  onMount(() => {
    setRatio(getSplitRatio(props.storageKey, defaultRatio));
  });

  function handlePointerDown(e: PointerEvent): void {
    const divider = e.currentTarget as HTMLElement;
    divider.setPointerCapture(e.pointerId);
    setDragging(true);
    document.body.style.userSelect = "none";
  }

  function handlePointerMove(e: PointerEvent): void {
    if (!dragging() || !containerEl) return;
    const rect = containerEl.getBoundingClientRect();
    const totalWidth = rect.width - 6; // subtract divider width
    const leftWidth = e.clientX - rect.left - 3; // center of divider

    // Enforce min widths
    const clampedLeft = Math.max(minLeftPx, Math.min(totalWidth - minRightPx, leftWidth));
    setRatio(clampedLeft / totalWidth);
  }

  function handlePointerUp(e: PointerEvent): void {
    if (!dragging()) return;
    const divider = e.currentTarget as HTMLElement;
    divider.releasePointerCapture(e.pointerId);
    setDragging(false);
    document.body.style.userSelect = "";
    setSplitRatio(props.storageKey, ratio());
  }

  /** Keyboard handler for arrow-key resizing of the split divider. */
  function handleKeyDown(e: KeyboardEvent): void {
    if (!containerEl) return;
    const step = 0.02; // 2% per key press
    let newRatio = ratio();
    if (e.key === "ArrowLeft") {
      newRatio = Math.max(minLeftPx / (containerEl.getBoundingClientRect().width - 6), newRatio - step);
      e.preventDefault();
    } else if (e.key === "ArrowRight") {
      newRatio = Math.min(1 - minRightPx / (containerEl.getBoundingClientRect().width - 6), newRatio + step);
      e.preventDefault();
    } else {
      return;
    }
    setRatio(newRatio);
    setSplitRatio(props.storageKey, newRatio);
  }

  return (
    <div class="split-pane" ref={containerEl}>
      <div class="split-left" style={{ "flex-basis": `${ratio() * 100}%` }}>
        {props.left}
      </div>

      <div
        class={`split-divider${dragging() ? " dragging" : ""}`}
        role="separator"
        aria-orientation="vertical"
        tabindex={0}
        aria-valuenow={Math.round(ratio() * 100)}
        aria-valuemin={0}
        aria-valuemax={100}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onKeyDown={handleKeyDown}
      ></div>

      <div class="split-right" style={{ "flex-basis": `${(1 - ratio()) * 100}%` }}>
        {props.right}
      </div>
    </div>
  );
};

export default SplitPane;
