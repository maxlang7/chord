#!/run/current-system/sw/bin/bash
set -euo pipefail
chord_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
unset TMPDIR TEMPDIR TMP TEMP NIX_BUILD_TOP
cd -- "$chord_root"
umask 077
mkdir -p private
# nix-shell launch.nix takes about a second before Python starts, and it only
# exists to put Wayland and GL on LD_LIBRARY_PATH. Remember that path and exec
# the client with the session Python so the keybind does not pay for Nix.
libs=private/gui-libs
exec >> private/desktop-client.log 2>&1
if [[ ! -s $libs ]]; then
  /run/current-system/sw/bin/nix-shell "$chord_root/launch.nix" --run 'printf %s "$LD_LIBRARY_PATH"' > "$libs"
fi
export LD_LIBRARY_PATH="$(<"$libs")${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
py=/etc/profiles/per-user/maxlang/bin/python3
if [[ ! -x $py ]]; then
  py=$(command -v python3)
fi
exec "$py" scripts/launch-client.py
