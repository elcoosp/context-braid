#!/bin/bash
set -euo pipefail
echo "=== 1. Socket get_dashboard ==="
python3 - << 'PY'
import socket
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(2)
s.connect("/tmp/braid.sock")
s.sendall(b'{"type":"get_dashboard"}\n')
print(s.recv(65536).decode("utf-8", errors="replace"))
PY
echo "=== 2. Logs braid daemon (/tmp/braidd.err.log) ==="
tail -n 30 /tmp/braidd.err.log 2>/dev/null || echo "(aucun log)"
echo "=== 3. Logs erreurs host (/tmp/braid-host.err.log) ==="
tail -n 15 /tmp/braid-host.err.log 2>/dev/null || echo "(aucune erreur)"
