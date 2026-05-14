import { listen } from "@tauri-apps/api/event";

type StatusEvent = {
  state: "starting" | "starting_sidecar" | "needs_model" | "ready" | "error";
  message?: string;
  path?: string;
};

const line = document.querySelector<HTMLParagraphElement>("#status-line");
const detail = document.querySelector<HTMLParagraphElement>("#status-detail");

const LABELS: Record<StatusEvent["state"], string> = {
  starting: "starting…",
  starting_sidecar: "loading model…",
  needs_model: "model required",
  ready: "ready",
  error: "error",
};

function render(s: StatusEvent) {
  if (line) line.textContent = LABELS[s.state] ?? s.state;
  if (detail) {
    const bits = [s.message, s.path].filter(Boolean);
    detail.textContent = bits.join("\n");
  }
}

await listen<StatusEvent>("status:update", (e) => render(e.payload));
