#!/usr/bin/env bash
set -euo pipefail

# Hive uninstaller

INSTALL_DIR="${HIVE_INSTALL_DIR:-$HOME/.local/bin}"

if [[ -f "${INSTALL_DIR}/hive" ]]; then
  rm "${INSTALL_DIR}/hive"
  echo "Removed ${INSTALL_DIR}/hive"
else
  echo "hive not found in ${INSTALL_DIR}"
fi
