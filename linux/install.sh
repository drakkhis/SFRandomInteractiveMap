#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}/applications"
BIN_DST="${BIN_DIR}/sat-rwg-map"
DESKTOP_DST="${APPS_DIR}/sat-rwg-map.desktop"

if [[ "${1:-}" != "--build" && -x "${REPO_ROOT}/sat-rwg-map" ]]; then
    BIN_SRC="${REPO_ROOT}/sat-rwg-map"
else
    BIN_SRC="${REPO_ROOT}/target/release/sat-rwg-map"
    if [[ "${1:-}" == "--build" || ! -x "${BIN_SRC}" ]]; then
        echo "Building release binary..."
        (cd "${REPO_ROOT}" && cargo build --release)
    fi
fi

mkdir -p "${BIN_DIR}" "${APPS_DIR}"
install -m 0755 "${BIN_SRC}" "${BIN_DST}"

sed "s|^Exec=.*$|Exec=${BIN_DST}|" "${SCRIPT_DIR}/sat-rwg-map.desktop" > "${DESKTOP_DST}"
chmod 0644 "${DESKTOP_DST}"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APPS_DIR}" >/dev/null 2>&1 || true
fi

echo "Installed binary : ${BIN_DST}"
echo "Installed desktop: ${DESKTOP_DST}"
echo "To uninstall     : rm -f \"${BIN_DST}\" \"${DESKTOP_DST}\""
