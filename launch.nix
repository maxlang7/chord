{ pkgs ? import <nixpkgs> {} }:
# Opening an already-built client must not download the development toolchain.
pkgs.mkShell {
  packages = [ pkgs.nodejs_24 pkgs.python3 ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    libxkbcommon wayland libGL libglvnd
    libX11 libXcursor libXi libXrandr libxcb
  ]);
}
