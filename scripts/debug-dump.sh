#!/bin/bash
set -euo pipefail
echo "=== 1. Dossiers détectés dans ~/Documents/Repos ==="
ls -1 "${HOME}/Documents/Repos"
echo ""
echo "=== 2. Derniers logs braid-host (/tmp/braid-host.log) ==="
tail -n 20 /tmp/braid-host.log 2>/dev/null || echo "(aucun log braid-host)"
echo ""
echo "=== 3. Derniers logs braidd (/tmp/braidd.err.log) ==="
tail -n 30 /tmp/braidd.err.log 2>/dev/null || echo "(aucun log braidd)"
