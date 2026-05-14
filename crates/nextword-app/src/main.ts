import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type Status =
  | "starting"
  | "starting_sidecar"
  | "needs_model"
  | "needs_ax_permission"
  | "ready"
  | "crashed"
  | "error";

type StatusEvent = {
  state: Status;
  message?: string;
  path?: string;
  fatal?: boolean;
  restart_attempt?: number;
  base_url?: string;
};

const LABELS: Record<Status, string> = {
  starting: "starting…",
  starting_sidecar: "loading model…",
  needs_model: "model required",
  needs_ax_permission: "accessibility access required",
  ready: "ready — start typing in any app",
  crashed: "inference service crashed",
  error: "error",
};

const lineEl = document.querySelector<HTMLParagraphElement>("#status-line");
const detailEl = document.querySelector<HTMLParagraphElement>("#status-detail");
const retryBtn = document.querySelector<HTMLButtonElement>("#retry-btn");

function render(s: StatusEvent) {
  if (lineEl) lineEl.textContent = LABELS[s.state] ?? s.state;
  if (detailEl) {
    const bits = [
      s.message,
      s.path,
      s.restart_attempt ? `restart attempt ${s.restart_attempt}` : null,
    ].filter(Boolean);
    detailEl.textContent = bits.join("\n");
  }
  if (retryBtn) {
    const showRetry =
      (s.state === "crashed" && s.fatal === true) || s.state === "error";
    retryBtn.hidden = !showRetry;
    retryBtn.disabled = false;
  }
}

retryBtn?.addEventListener("click", async () => {
  if (!retryBtn) return;
  retryBtn.disabled = true;
  try {
    await invoke("cmd_retry_sidecar");
  } catch (err) {
    if (detailEl) detailEl.textContent = `retry failed: ${String(err)}`;
    retryBtn.disabled = false;
  }
});

await listen<StatusEvent>("status:update", (e) => render(e.payload));
