{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell { packages = with pkgs; [ nodejs_24 python3 grim cargo rustc pkg-config openssl ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    libxkbcommon wayland libGL libglvnd
    xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb
  ]);
  shellHook = ''
    if [ ! -d .venv ]; then python -m venv .venv; fi
    source .venv/bin/activate
  '';
}
