#!/usr/bin/env bash
# Builds FlowerBox in release mode for Linux and Windows (cross-compiled with mingw) and packs each
# with its assets and a README into dist/ (folders + flowerbox-linux.tar.gz / flowerbox-windows.zip).
# Needs: rustup target add x86_64-pc-windows-gnu; the mingw-w64 toolchain (x86_64-w64-mingw32-gcc); zip.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
cargo build --release --target x86_64-pc-windows-gnu
dist=dist
mkdir -p "$dist"
for os in linux windows; do
    out="$dist/flowerbox-$os"
    mkdir -p "$out"
    rm -rf -- "${out:?}/assets"
    cp -r assets "$out/assets"
    rm -f -- "$out/assets/levels/"*.png  # design plans, not needed to play
    cp tools/README-dist.txt "$out/README.txt"
done
cp target/release/flowerbox "$dist/flowerbox-linux/"
cp target/x86_64-pc-windows-gnu/release/flowerbox.exe "$dist/flowerbox-windows/"
(cd "$dist" && tar -czf flowerbox-linux.tar.gz flowerbox-linux && rm -f flowerbox-windows.zip && zip -qr flowerbox-windows.zip flowerbox-windows)
ls -lh "$dist"
