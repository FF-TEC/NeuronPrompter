// TypeScript declaration for wry's IPC bridge.
// When running inside a native tao/wry window, window.ipc is injected
// by the WebView runtime. In browser mode, it is undefined.
declare global {
  interface Window {
    ipc?: {
      postMessage(message: string): void;
    };
  }
}

export {};
