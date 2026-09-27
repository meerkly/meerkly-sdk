#!/usr/bin/env bash
# Build the Android SDK: cross-compile the uniffi cdylib for every shipped ABI,
# regenerate the Kotlin bindings, assemble the AAR, and prove the AAR actually
# contains what it is supposed to.
#
# This is the single entry point used by a developer, by ci.yml, and by
# release-sdks.yml, so the three cannot drift apart.
#
# Requires: the Android NDK, cargo-ndk, and the three Android Rust targets:
#   sdkmanager --install "ndk;28.2.13676358"
#   cargo install cargo-ndk
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

ANDROID_DIR="sdk/android"
JNI_LIBS="$ANDROID_DIR/lib/src/main/jniLibs"
KOTLIN_OUT="$ANDROID_DIR/lib/src/main/kotlin"
KOTLIN_PKG_DIR="$KOTLIN_OUT/com/meerkly/sdk"
ABIS=(arm64-v8a armeabi-v7a x86_64)
# Must match `minSdk` in sdk/android/lib/build.gradle.kts. cargo-ndk defaults to
# 21, which would build against an older libc than the AAR claims to support.
MIN_SDK=24

die() { echo "error: $*" >&2; exit 1; }

# ---- toolchain ------------------------------------------------------------
# Checked before cargo runs, so a missing NDK costs a second rather than a
# confusing linker error several minutes in.
command -v cargo-ndk >/dev/null \
  || die "cargo-ndk is not installed — cargo install cargo-ndk"

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  sdk_root="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
  if [ -d "$sdk_root/ndk" ]; then
    # Highest installed version wins; pin with ANDROID_NDK_HOME to override.
    ANDROID_NDK_HOME="$(find "$sdk_root/ndk" -maxdepth 1 -mindepth 1 -type d \
      | sort -V | tail -1)"
  fi
fi
[ -n "${ANDROID_NDK_HOME:-}" ] && [ -d "${ANDROID_NDK_HOME:-}" ] \
  || die "no Android NDK found — set ANDROID_NDK_HOME, or install one with:
  sdkmanager --install \"ndk;28.2.13676358\""
export ANDROID_NDK_HOME

# Gradle needs the SDK too, via ANDROID_HOME or a local.properties we do not commit.
if [ -z "${ANDROID_HOME:-}" ]; then
  export ANDROID_HOME="${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}"
fi
[ -d "$ANDROID_HOME" ] || die "ANDROID_HOME does not exist: $ANDROID_HOME"

echo "== NDK  $ANDROID_NDK_HOME"
echo "== SDK  $ANDROID_HOME"

# ---- 1. cross-compile the cdylib for each ABI -----------------------------
# Removed first so an ABI dropped from the list cannot linger in a developer's
# tree and get packaged.
rm -rf "$JNI_LIBS"
ndk_targets=()
for abi in "${ABIS[@]}"; do ndk_targets+=(-t "$abi"); done
echo "== cross-compiling libmeerkly.so for ${ABIS[*]} (API $MIN_SDK)"
cargo ndk "${ndk_targets[@]}" -P "$MIN_SDK" -o "$JNI_LIBS" \
  build --release -p meerkly-client-ffi

# ---- 2. regenerate the Kotlin bindings ------------------------------------
# From the *host* cdylib: uniffi reads the component's metadata out of it, which
# is identical across targets, and building for the host is what the generator
# binary already needs anyway.
echo "== generating Kotlin bindings"
cargo build --release -p meerkly-client-ffi
LIB=""
for cand in target/release/libmeerkly.dylib target/release/libmeerkly.so; do
  [ -f "$cand" ] && LIB="$cand" && break
done
[ -n "$LIB" ] || die "host cdylib not found under target/release/"
rm -rf "$KOTLIN_PKG_DIR"
cargo run --release -q -p meerkly-client-ffi --bin uniffi-bindgen -- \
  generate --library "$LIB" --language kotlin --out-dir "$KOTLIN_OUT"
[ -f "$KOTLIN_PKG_DIR/meerkly.kt" ] \
  || die "expected generated Kotlin at $KOTLIN_PKG_DIR/meerkly.kt — is package_name still com.meerkly.sdk in crates/client-ffi/uniffi.toml?"

# ---- 3. assemble the AAR --------------------------------------------------
echo "== assembling the AAR"
(cd "$ANDROID_DIR" && ./gradlew --console=plain assembleRelease)

AAR="$(find "$ANDROID_DIR/lib/build/outputs/aar" -name '*-release.aar' | head -1)"
[ -n "$AAR" ] || die "no AAR produced under $ANDROID_DIR/lib/build/outputs/aar"

# ---- 4. content check -----------------------------------------------------
# The AAR is what gets uploaded, and a Maven Central release cannot be
# withdrawn. So verify the artifact itself rather than trusting that the build
# steps above did what they claimed: a missing ABI or a bindings file that never
# made it into classes.jar are both silent failures at this point.
echo "== verifying $AAR"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
unzip -q -o "$AAR" -d "$tmp"

for abi in "${ABIS[@]}"; do
  [ -f "$tmp/jni/$abi/libmeerkly.so" ] \
    || die "AAR is missing jni/$abi/libmeerkly.so"
done
packaged="$(cd "$tmp/jni" && ls -d */ 2>/dev/null | tr -d '/' | sort | tr '\n' ' ')"
expected="$(printf '%s\n' "${ABIS[@]}" | sort | tr '\n' ' ')"
[ "$packaged" = "$expected" ] \
  || die "AAR packages ABIs [$packaged] but should package exactly [$expected]"

[ -f "$tmp/classes.jar" ] || die "AAR has no classes.jar"
unzip -l "$tmp/classes.jar" | grep -q 'com/meerkly/sdk/ProxyClient\.class' \
  || die "classes.jar does not contain com/meerkly/sdk/ProxyClient.class"

# Actual file size, not `du` — du reports allocated disk blocks, which vary
# between otherwise identical builds and makes the number look unstable.
size="$(awk -v b="$(wc -c < "$AAR")" 'BEGIN { printf "%.1fM", b / 1048576 }')"
echo
echo "ok — $AAR ($size)"
echo "   ABIs:    ${ABIS[*]}"
echo "   classes: com.meerkly.sdk"
