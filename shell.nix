{ pkgs ? import <nixpkgs> {} }:

# Dev shell that runs the Nix-built Electron, whose version matches the
# `electron` devDependency. The npm `electron` package (used by `npm run dev`
# and Playwright) resolves its binary through ELECTRON_OVERRIDE_DIST_PATH.
# xvfb-run gives the e2e tests an offscreen X display.

pkgs.mkShell {
  packages = with pkgs; [ nodejs electron_44 xvfb-run ];

  ELECTRON_SKIP_BINARY_DOWNLOAD = "1";
  ELECTRON_OVERRIDE_DIST_PATH = "${pkgs.electron_44}/bin";
}
