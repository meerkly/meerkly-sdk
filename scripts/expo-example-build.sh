#!/usr/bin/env bash
# Build the Expo example app and assert the manifest came out right.
#
#   ./scripts/expo-example-build.sh default      in-process mode
#   ./scripts/expo-example-build.sh background   foreground service opted in
#
# This is what actually proves the config plugin works. The plugin's unit tests
# check the object it produces; only a real prebuild plus a real manifest merge
# shows that an app which did NOT opt in stays free of a specialUse service
# declaration — the promise the whole design rests on.
set -euo pipefail

mode="${1:-default}"
case "$mode" in
  default|background) ;;
  *) echo "usage: $0 [default|background]" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root/sdk/expo/example"

if [ -z "${ANDROID_HOME:-}" ]; then
  export ANDROID_HOME="${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}"
fi
[ -d "$ANDROID_HOME" ] || { echo "error: ANDROID_HOME does not exist: $ANDROID_HOME" >&2; exit 1; }

# Expo and React Native target JDK 17, and CI pins it via setup-java. Newer
# JDKs are simply untested by upstream, and the failures they produce in Expo's
# Gradle plugin are opaque, so refuse early with the fix rather than let someone
# spend an afternoon on a stack trace. Developer machines only.
jdk_major="$("${JAVA_HOME:+$JAVA_HOME/bin/}java" -version 2>&1 \
  | sed -n 's/.*version "\([0-9]*\).*/\1/p' | head -1)"
if [ -n "$jdk_major" ] && [ "$jdk_major" -gt 21 ] 2>/dev/null; then
  echo "error: JDK $jdk_major is newer than Expo and React Native support. Use JDK 17:" >&2
  echo "         brew install openjdk@17" >&2
  echo "         export JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home" >&2
  exit 1
fi

# app.plugin.js loads plugin/build, which only exists once the TypeScript has
# been compiled. A published package ships it (prepublishOnly builds it), but
# from a source checkout it has to be built first or prebuild dies resolving the
# plugin — with a Node stack trace that never mentions the real cause.
if [ ! -d "$root/sdk/expo/node_modules" ]; then
  echo "error: sdk/expo dependencies are not installed. Run:" >&2
  echo "         (cd sdk/expo && npm install --legacy-peer-deps)" >&2
  exit 1
fi
echo "== building the module and its config plugin"
(cd "$root/sdk/expo" && npm run --silent build)

echo "== prebuild ($mode)"
if [ "$mode" = background ]; then export MEERKLY_BACKGROUND=1; else unset MEERKLY_BACKGROUND; fi
npx expo prebuild --platform android --clean --no-install

# The module depends on com.meerkly:sdk at the release version, which during its
# own release run has not reached Maven Central yet. Resolving from the local
# repository the AAR build just populated is what lets this run before the
# artifact is public — the same ordering problem the Rust crates have.
echo "== allowing resolution from the local Maven repository"
perl -0pi -e 's/(allprojects \{\n\s*repositories \{\n)/$1    mavenLocal()\n/' android/build.gradle
grep -q 'mavenLocal()' android/build.gradle || { echo "error: could not add mavenLocal to android/build.gradle" >&2; exit 1; }

manifest=android/app/src/main/AndroidManifest.xml
echo "== checking the merged manifest"
if [ "$mode" = background ]; then
  grep -q 'com.meerkly.expo.MeerklyService' "$manifest" \
    || { echo "error: background build is missing the service declaration" >&2; exit 1; }
  grep -q 'android:foregroundServiceType="specialUse"' "$manifest" \
    || { echo "error: the service is declared without the specialUse type" >&2; exit 1; }
  grep -q 'FOREGROUND_SERVICE_SPECIAL_USE' "$manifest" \
    || { echo "error: the specialUse permission is missing" >&2; exit 1; }
  echo "   service + specialUse type + permissions present, as expected"
else
  # The one that matters. An app that never asked for background work must not
  # inherit a service it has to justify at Play Store review.
  if grep -q 'com.meerkly.expo.MeerklyService' "$manifest"; then
    echo "error: the foreground service leaked into a build that did not opt in" >&2
    exit 1
  fi
  if grep -q 'FOREGROUND_SERVICE' "$manifest"; then
    echo "error: foreground service permissions leaked into a build that did not opt in" >&2
    exit 1
  fi
  echo "   no service and no foreground permissions, as expected"
fi

echo "== assembling"
# One ABI, not the four gradle.properties lists. With the New Architecture on,
# every ABI compiles React Native's C++ in parallel across all host cores, and
# CI runs on a self-hosted runner capped at 8Gi: four ABIs peaked at 8110Mi and
# were OOM-killed (exit 137) three runs out of three on 2026-09-12. Nothing this
# script checks depends on the ABI — the manifest assertions above run before
# assembly — so one proves the same thing. Passed as -P because prebuild
# --clean regenerates android/ and would wipe an edit to gradle.properties.
(cd android && ./gradlew --console=plain -PreactNativeArchitectures=arm64-v8a :app:assembleDebug)

apk="android/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] || { echo "error: no APK produced at $apk" >&2; exit 1; }
echo
echo "ok — $mode build: $apk"
