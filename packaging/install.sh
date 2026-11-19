#!/bin/sh
# Install a built mpc-rv into a directory tree, the same way for every package (.deb, AppImage, AUR).
#
#   packaging/install.sh <mpc-rv binary> <prefix>      e.g. packaging/install.sh target/release/mpc-rv "$pkgdir/usr"
#
# Run it from the repository root. The program finds its translations in <prefix>/share/mpc-rv/po
# (next to bin/), so the tree can live anywhere.
set -eu

BIN=$1
PREFIX=$2
APP=io.github.mpcrv.Player

install -Dm755 "$BIN" "$PREFIX/bin/mpc-rv"
install -Dm644 packaging/$APP.desktop "$PREFIX/share/applications/$APP.desktop"
install -Dm644 packaging/$APP.metainfo.xml "$PREFIX/share/metainfo/$APP.metainfo.xml"
for s in 48 128 256; do
    install -Dm644 mpc-rv/res/icons/$s.png "$PREFIX/share/icons/hicolor/${s}x$s/apps/$APP.png"
done
install -d "$PREFIX/share/mpc-rv/po"
install -m644 mpc-rv/res/po/*.po "$PREFIX/share/mpc-rv/po/"
