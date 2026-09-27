#!/usr/bin/env bash
# Build the uniffi cdylib and (re)generate every FFI language binding from it:
# Python, Swift, Kotlin (via the crate's uniffi-bindgen), and Go (via
# uniffi-bindgen-go). The uniffi namespace is `meerkly`, so bindings are
# `import meerkly` (Python), package `meerkly` (Go), `uniffi.meerkly` (Kotlin).
# Host artifact only — CI regenerates per target, including the Android NDK ABIs.
#
# Requires: uniffi-bindgen-go (cargo install uniffi-bindgen-go
#   --git https://github.com/NordSecurity/uniffi-bindgen-go --tag v0.4.0+v0.28.3)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release -p meerkly-client-ffi
LIB=""
for cand in target/release/libmeerkly.dylib target/release/libmeerkly.so; do
  [ -f "$cand" ] && LIB="$cand" && break
done
[ -n "$LIB" ] || { echo "cdylib not found under target/release/" >&2; exit 1; }

gen() {
  cargo run --release -q -p meerkly-client-ffi --bin uniffi-bindgen -- \
    generate --library "$LIB" --language "$1" --out-dir "$2"
}
gen python sdk/python
gen swift  sdk/swift
gen kotlin sdk/android/lib/src/main/kotlin
gen ruby   sdk/ruby
# Python and Ruby load the native lib from their own directory / lib path.
cp "$LIB" sdk/python/
cp "$LIB" sdk/ruby/

# Go uses a separate generator.
if command -v uniffi-bindgen-go >/dev/null; then
  uniffi-bindgen-go --library "$LIB" --out-dir sdk/go
  # uniffi-bindgen-go 0.4.0 returns a typed-nil *ProxyError as `error` from async
  # methods, so a *successful* Start()/Stop() would look like a failure. Nil-check it.
  perl -i -pe 's/^\treturn err$/\tif err != nil {\n\t\treturn err\n\t}\n\treturn nil/' \
    sdk/go/meerkly/meerkly.go
else
  echo "warning: uniffi-bindgen-go not installed — skipped Go generation" >&2
fi

echo "regenerated FFI bindings: python, swift, kotlin, ruby$(command -v uniffi-bindgen-go >/dev/null && echo ', go')"
