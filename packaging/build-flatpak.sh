#!/bin/sh
# Build mpc-rv as a Flatpak and package it into one installable file, dist/mpc-rv-<version>-x86_64.flatpak.
#
#   packaging/build-flatpak.sh            (from the repository root)
#
# Needs flatpak and flatpak-builder. The first run downloads the GNOME runtime and SDK (a few GB)
# and builds libmpv and FFmpeg, which takes a while; later runs reuse that work.
# Install the result with:  flatpak install --user dist/mpc-rv-<version>-x86_64.flatpak
set -eu

cd "$(dirname "$0")/.."
APP=io.github.mpcrv.Player
MANIFEST=packaging/flatpak/$APP.yml
WORK=.flatpak-build # build folders (gitignored)

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

flatpak-builder --user --install-deps-from=flathub --force-clean --ccache --disable-rofiles-fuse \
    --state-dir="$WORK/state" --repo="$WORK/repo" "$WORK/build" "$MANIFEST"

# One file with the app; on install it fetches the GNOME runtime from Flathub if it's missing.
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' mpc-rv/Cargo.toml | head -1)
OUT=dist/mpc-rv-$VERSION-x86_64.flatpak
mkdir -p dist
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$WORK/repo" "$OUT" "$APP"

echo "Built $OUT ($(du -h "$OUT" | cut -f1))"
