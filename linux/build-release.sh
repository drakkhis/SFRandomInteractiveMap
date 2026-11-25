#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${REPO_ROOT}/dist"

DESKTOP_PKG="SF-Random-Interactive-Map-linux-x86_64"
DESKTOP_STAGE="${DIST_DIR}/${DESKTOP_PKG}"
DESKTOP_ARCHIVE="${DIST_DIR}/${DESKTOP_PKG}.tar.gz"

HEADLESS_PKG="SF-Random-Interactive-Map-linux-x86_64-headless"
HEADLESS_STAGE="${DIST_DIR}/${HEADLESS_PKG}"
HEADLESS_ARCHIVE="${DIST_DIR}/${HEADLESS_PKG}.tar.gz"
HEADLESS_TARGET_DIR="${REPO_ROOT}/target/headless"

echo "Building sat-rwg-map (Linux desktop release)..."
(cd "${REPO_ROOT}" && cargo build --release)

echo "Building sat-rwg-map (Linux headless release)..."
(cd "${REPO_ROOT}" && cargo build --release --no-default-features --target-dir "${HEADLESS_TARGET_DIR}")

rm -rf "${DESKTOP_STAGE}" "${HEADLESS_STAGE}"
mkdir -p "${DESKTOP_STAGE}/linux" "${HEADLESS_STAGE}" "${DIST_DIR}"

install -m 0755 "${REPO_ROOT}/target/release/sat-rwg-map" "${DESKTOP_STAGE}/sat-rwg-map"
install -m 0644 "${REPO_ROOT}/README.md" "${DESKTOP_STAGE}/README.md"
install -m 0644 "${REPO_ROOT}/linux/sat-rwg-map.desktop" "${DESKTOP_STAGE}/linux/sat-rwg-map.desktop"
install -m 0755 "${REPO_ROOT}/linux/install.sh" "${DESKTOP_STAGE}/linux/install.sh"

install -m 0755 "${HEADLESS_TARGET_DIR}/release/sat-rwg-map" "${HEADLESS_STAGE}/sat-rwg-map"
install -m 0644 "${REPO_ROOT}/README.md" "${HEADLESS_STAGE}/README.md"

tar -C "${DIST_DIR}" -czf "${DESKTOP_ARCHIVE}" "${DESKTOP_PKG}"
tar -C "${DIST_DIR}" -czf "${HEADLESS_ARCHIVE}" "${HEADLESS_PKG}"
rm -rf "${DESKTOP_STAGE}" "${HEADLESS_STAGE}"

(cd "${DIST_DIR}" && sha256sum "${DESKTOP_PKG}.tar.gz" "${HEADLESS_PKG}.tar.gz" > SHA256SUMS.txt)

echo "Created Linux desktop archive : ${DESKTOP_ARCHIVE}"
echo "Created Linux headless archive: ${HEADLESS_ARCHIVE}"
echo "Created checksums             : ${DIST_DIR}/SHA256SUMS.txt"
