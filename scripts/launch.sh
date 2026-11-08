#!/run/current-system/sw/bin/bash
set -euo pipefail
chord_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
unset TMPDIR TEMPDIR TMP TEMP NIX_BUILD_TOP
cd -- "$chord_root"
umask 077
mkdir -p private
exec /run/current-system/sw/bin/nix-shell "$chord_root/shell.nix" \
  --run 'exec python scripts/launch-client.py' >> private/desktop-client.log 2>&1
