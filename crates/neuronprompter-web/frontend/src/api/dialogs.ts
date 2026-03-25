/**
 * Native file dialog bridge.
 *
 * In desktop mode (wry), calls the web endpoint which opens a native OS dialog
 * via the rfd crate. In browser mode, returns null (caller should use
 * browser-native download/upload instead).
 */

const BASE = "/api/v1/web/dialog";

interface DialogFilter {
  name: string;
  extensions: string[];
}

interface DialogRequest {
  title?: string;
  filters?: DialogFilter[];
  default_name?: string;
}

interface DialogResult {
  path: string | null;
}

async function postDialog(endpoint: string, body: DialogRequest = {}): Promise<string | null> {
  try {
    const res = await fetch(`${BASE}/${endpoint}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) return null;
    const result: DialogResult = await res.json();
    return result.path;
  } catch {
    return null;
  }
}

/** Opens a native save-file dialog. Returns the chosen path or null. */
export async function showSaveDialog(
  title: string,
  filters: DialogFilter[],
  defaultName?: string,
): Promise<string | null> {
  return postDialog("save", { title, filters, default_name: defaultName });
}

/** Opens a native open-file dialog. Returns the chosen path or null. */
export async function showOpenFileDialog(
  title: string,
  filters: DialogFilter[],
): Promise<string | null> {
  return postDialog("open-file", { title, filters });
}

/** Opens a native directory picker. Returns the chosen path or null. */
export async function showOpenDirDialog(): Promise<string | null> {
  return postDialog("open-dir");
}
