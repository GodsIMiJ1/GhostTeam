#!/usr/bin/env bash
set -euo pipefail

INSTALL_BIN="/usr/local/bin/ghostteam"
GHOSTTEAM_HOME="${HOME}/.ghostteam"

rm -f "${INSTALL_BIN}"
echo "Removed GhostTeam binary at ${INSTALL_BIN}"

if [[ -d "${GHOSTTEAM_HOME}" ]]; then
  read -r -p "Delete ${GHOSTTEAM_HOME}? [y/N] " answer
  case "${answer}" in
    [yY][eE][sS]|[yY])
      rm -rf "${GHOSTTEAM_HOME}"
      echo "Removed GhostTeam workspace at ${GHOSTTEAM_HOME}"
      ;;
    *)
      echo "Kept GhostTeam workspace at ${GHOSTTEAM_HOME}"
      ;;
  esac
fi
