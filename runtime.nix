{ pkgs ? import <nixpkgs> {} }:
let
  whatsapp = pkgs.mautrix-whatsapp.override { withGoolm = true; };
  slack = (pkgs.mautrix-slack.override { withGoolm = true; }).overrideAttrs (old: {
    patches = (old.patches or []) ++ [ ./patches/slack-login-version-data.patch ];
  });
  discord = pkgs.mautrix-discord.overrideAttrs (old: {
    buildInputs = []; tags = [ "nocrypto" ];
    patches = (old.patches or []) ++ [ ./patches/discord-empty-history.patch ];
  });
  python = pkgs.python3.withPackages (ps: [ ps.pip ps.pyyaml ]);
in pkgs.mkShell {
  packages = [ pkgs.nodejs_24 pkgs.matrix-synapse whatsapp slack discord python ];
  shellHook = ''
    export CHORD_DISCORD_EXAMPLE=${pkgs.mautrix-discord.src}/example-config.yaml
    if [ ! -d .venv ]; then python -m venv --system-site-packages .venv; fi
    source .venv/bin/activate
  '';
}
