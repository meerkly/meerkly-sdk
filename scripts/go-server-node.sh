#!/usr/bin/env bash
# Build and run the Go always-on exit node (sdk/go/cmd/server-node). It links the
# uniffi cdylib, so this builds meerkly-client-ffi (release) and sets the CGO
# flags first. Configure via the environment:
#
#   MEERKLY_PUBLISHER_ID       (required) public publisher id
#   MEERKLY_GATEWAY_ADDRESSES  (required) host:port[,host:port...]
#   MEERKLY_CA_CERT_PATH       (default certs/dev/ca.crt)
#
# Pass `build -o <path>` as args to produce a binary instead of running.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release -p meerkly-client-ffi
export CGO_CFLAGS="-I$root/sdk/go/meerkly"
export CGO_LDFLAGS="-L$root/target/release -lmeerkly -Wl,-rpath,$root/target/release"

cd sdk/go
if [ "${1:-run}" = "build" ]; then
  shift
  exec go build "$@" ./cmd/server-node
fi
exec go run ./cmd/server-node
