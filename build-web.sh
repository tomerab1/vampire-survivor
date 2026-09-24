#!/usr/bin/env bash
# Builds the wasm version into web/dist. Serve it with: cargo run -p serve
set -euo pipefail
cd "$(dirname "$0")"
PROFILE="${PROFILE:-wasm-release}"
cargo build --profile "$PROFILE" --target wasm32-unknown-unknown
rm -rf web/dist && mkdir -p web/dist
wasm-bindgen --target web --no-typescript --out-dir web/dist \
  "target/wasm32-unknown-unknown/$PROFILE/zombie-survivor.wasm"
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o web/dist/zombie-survivor_bg.wasm web/dist/zombie-survivor_bg.wasm
fi
cp web/index.html web/dist/
cp -R assets web/dist/assets
du -sh web/dist/zombie-survivor_bg.wasm
