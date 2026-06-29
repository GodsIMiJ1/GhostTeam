#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_NAME="ghostteam"
INSTALL_DIR="/usr/local/bin"
INSTALL_BIN="${INSTALL_DIR}/${BIN_NAME}"
GHOSTTEAM_HOME="${HOME}/.ghostteam"

mkdir -p "${GHOSTTEAM_HOME}"

cd "${ROOT_DIR}"
cargo build --release

install -m 755 "target/release/${BIN_NAME}" "${INSTALL_BIN}"
echo "Installed ${BIN_NAME} by GodsIMiJ AI Solutions Inc. to ${INSTALL_BIN}"
echo "Workspace directory for GhostTeam by James D. Ingersoll: ${GHOSTTEAM_HOME}"
