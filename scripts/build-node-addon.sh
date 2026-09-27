#!/usr/bin/env bash
# Build the @meerkly/sdk napi addon for the HOST platform (dev + the node smoke).
#
# Uses @napi-rs/cli, the same tool the release workflow uses — it compiles
# crates/client-napi and emits `meerkly-sdk.<host-triple>.node` plus the generated
# `index.js` loader and `index.d.ts`. The multi-platform build + npm publish lives
# in .github/workflows/release-sdk-node.yml.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root/sdk/node"

# @napi-rs/cli is a devDependency; install it if missing. --omit=optional skips the
# per-platform packages (not published yet, and unnecessary — the local .node is used).
[ -x node_modules/.bin/napi ] || npm install --omit=optional --no-audit --no-fund

profile="${1:-release}"
flag="--release"; [ "$profile" = "release" ] || flag=""

npx napi build $flag --platform \
  --manifest-path ../../crates/client-napi/Cargo.toml \
  --output-dir .

echo "built $(ls meerkly-sdk.*.node 2>/dev/null | head -1) + index.js/index.d.ts"
