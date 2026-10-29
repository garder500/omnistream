// Notifications système (plugin Tauri). Silencieux hors Tauri, dans le dock, ou si l'app a le focus.
export const isDock = () => new URLSearchParams(window.location.search).has("dock");

let granted: Promise<boolean> | null = null;

async function ensurePermission(): Promise<boolean> {
  const { isPermissionGranted, requestPermission } = await import("@tauri-apps/plugin-notification");
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

// Au plus une notification toutes les 5 s : un raid ou une vague de spam ne doit pas inonder le centre de notifications.
const MIN_GAP_MS = 5000;
let lastAt = 0;

export async function systemNotify(title: string, body: string): Promise<void> {
  if (isDock() || document.hasFocus()) return;
  if (Date.now() - lastAt < MIN_GAP_MS) return;
  lastAt = Date.now();
  try {
    granted ??= ensurePermission();
    if (!(await granted)) return;
    const { sendNotification } = await import("@tauri-apps/plugin-notification");
    sendNotification({ title, body: body.slice(0, 200) });
  } catch {
    granted = null; // hors Tauri ou permission indisponible : on ignore
  }
}
