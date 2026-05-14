import { listen } from "@tauri-apps/api/event";

type ShowPayload = { words: string[]; x: number; y: number };

const chips = Array.from(document.querySelectorAll<HTMLButtonElement>(".chip"));

function show(words: string[]) {
  chips.forEach((chip, i) => {
    const wordEl = chip.querySelector<HTMLSpanElement>(".word");
    const word = words[i] ?? "";
    if (wordEl) wordEl.textContent = word;
    chip.style.display = word ? "inline-flex" : "none";
    chip.dataset.active = i === 0 ? "true" : "false";
  });
  document.body.style.opacity = "1";
}

function hide() {
  document.body.style.opacity = "0";
}

await listen<ShowPayload>("suggestions:show", (e) => show(e.payload.words));
await listen("suggestions:hide", () => hide());

hide();
