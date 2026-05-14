#!/usr/bin/env bash
# Downloads a small GGUF into the per-user models dir so M1 can boot
# the sidecar without waiting on the full Llama-3.2-1B download.
#
# Usage: ./scripts/dev-download-test-model.sh
set -euo pipefail

MODELS_DIR="${HOME}/Library/Application Support/NextWord/models"
mkdir -p "${MODELS_DIR}"

MODEL_NAME="Qwen2.5-0.5B-Instruct-Q4_K_M.gguf"
MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf"
DEST="${MODELS_DIR}/${MODEL_NAME}"

if [ -f "${DEST}" ]; then
  echo "already downloaded: ${DEST}"
  exit 0
fi

echo "Downloading ${MODEL_NAME}..."
curl -L --fail --progress-bar -o "${DEST}.tmp" "${MODEL_URL}"
mv "${DEST}.tmp" "${DEST}"
echo "Saved to ${DEST}"

# Also symlink the default Llama path so the app finds something.
LLAMA_PATH="${MODELS_DIR}/Llama-3.2-1B-Instruct-Q4_K_M.gguf"
if [ ! -e "${LLAMA_PATH}" ]; then
  ln -s "${DEST}" "${LLAMA_PATH}"
  echo "Symlinked ${LLAMA_PATH} -> ${DEST}"
fi
