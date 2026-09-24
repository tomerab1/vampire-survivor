#!/usr/bin/env bash
# Publishes web/dist to the gh-pages branch (GitHub Pages serves it at /<repo>/).
# Run ./build-web.sh first.
set -euo pipefail
cd "$(dirname "$0")"
REMOTE="$(git remote get-url origin)"
[ -f web/dist/index.html ] || { echo "web/dist missing; run ./build-web.sh" >&2; exit 1; }
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R web/dist/. "$STAGE/"
touch "$STAGE/.nojekyll"
git -C "$STAGE" init -q -b gh-pages
git -C "$STAGE" add -A
git -C "$STAGE" -c user.name="$(git config user.name)" -c user.email="$(git config user.email)" \
  commit -q -m "Deploy web build $(git rev-parse --short HEAD)"
git -C "$STAGE" push -q -f "$REMOTE" gh-pages
echo "pushed gh-pages from $(git rev-parse --short HEAD)"
