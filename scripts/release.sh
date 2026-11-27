#!/bin/sh
set -eu

fail() {
  echo "release: $*" >&2
  exit 1
}

stage="${1:-all}"
case "$stage" in
  build | sign | all) ;;
  *) fail "usage: release.sh [build|sign]" ;;
esac
builds() { [ "$stage" != sign ]; }
signs() { [ "$stage" != build ]; }

cd "$(git rev-parse --show-toplevel)"

[ "$(uname -s)" = Darwin ] || fail "Raiden builds only on macOS."
if builds; then
  [ -z "$(git status --porcelain)" ] \
    || fail "commit or remove every change first, so the download matches a commit."
fi

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
[ -n "$version" ] || fail "could not read the version from Cargo.toml."

target="${CARGO_TARGET_DIR:-target}"
app="$target/release/bundle/macos/Raiden.app"
dist="$target/dist"
name="Raiden-$(uname -m).dmg"

if signs; then
  identity="${RAIDEN_SIGNING_IDENTITY:-}"
  if [ -z "$identity" ]; then
    found="$(security find-identity -v -p codesigning | grep '"Developer ID Application: ' | awk '{print $2}' | sort -u || true)"
    case "$(printf '%s' "$found" | grep -c .)" in
      0) fail "no Developer ID Application certificate in your keychains. Create one in Xcode: Settings, Accounts, Manage Certificates." ;;
      1) identity="$found" ;;
      *) fail "more than one Developer ID Application certificate. Choose one with RAIDEN_SIGNING_IDENTITY." ;;
    esac
  fi

  if [ -n "${NOTARY_KEY_PATH:-}" ]; then
    [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_ISSUER:-}" ] \
      || fail "NOTARY_KEY_PATH needs NOTARY_KEY_ID and NOTARY_ISSUER."
    set -- --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER"
  else
    set -- --keychain-profile "${RAIDEN_NOTARY_PROFILE:-raiden}"
    xcrun notarytool history "$@" > /dev/null \
      || fail "no notary login saved as ${RAIDEN_NOTARY_PROFILE:-raiden}. Save one with: xcrun notarytool store-credentials ${RAIDEN_NOTARY_PROFILE:-raiden}"
  fi
fi

if builds; then
  echo "Building Raiden $version."
  env -u APPLE_CERTIFICATE -u APPLE_SIGNING_IDENTITY -u APPLE_ID -u APPLE_PASSWORD \
    -u APPLE_API_KEY -u APPLE_API_KEY_PATH -u APPLE_API_ISSUER \
    ui/node_modules/.bin/tauri build --bundles app --no-sign
  [ -d "$app" ] || fail "the build made no $app."
fi

signs || exit 0
[ -d "$app" ] || fail "there is no $app to sign. Build it first with: release.sh build"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

notarize() {
  file="$1"
  shift
  echo "Sending $(basename "$file") to Apple to notarize. This usually takes a few minutes."
  xcrun notarytool submit "$file" "$@" --wait --timeout 1h --output-format plist \
    > "$work/submission.plist" || true
  id="$(plutil -extract id raw "$work/submission.plist" 2>/dev/null)" \
    || fail "Apple could not take $(basename "$file")."
  status="$(plutil -extract status raw "$work/submission.plist" 2>/dev/null || echo unknown)"
  if [ "$status" != Accepted ]; then
    xcrun notarytool log "$id" "$@" >&2 || true
    fail "Apple did not notarize $(basename "$file"): $status. Its log is above."
  fi
}

staple() {
  tries=0
  until xcrun stapler staple "$1"; do
    tries=$((tries + 1))
    [ "$tries" -lt 5 ] || fail "could not staple Apple's ticket to $(basename "$1")."
    sleep 15
  done
  xcrun stapler validate "$1"
}

notarized() {
  file="$1"
  shift
  verdict="$(spctl --assess "$@" "$file" 2>&1)" || fail "Gatekeeper refused $(basename "$file"): $verdict"
  case "$verdict" in
    *"source=Notarized Developer ID"*) echo "$verdict" ;;
    *) fail "Gatekeeper does not see $(basename "$file") as notarized: $verdict" ;;
  esac
}

codesign --force --options runtime --timestamp --sign "$identity" "$app"
codesign --verify --strict --verbose=2 "$app"

ditto -c -k --keepParent "$app" "$work/Raiden.zip"
notarize "$work/Raiden.zip" "$@"
staple "$app"

mkdir -p "$work/disk" "$dist"
ditto "$app" "$work/disk/Raiden.app"
ln -s /Applications "$work/disk/Applications"
rm -f "$dist/$name" "$dist/$name.sha256"
tries=0
until hdiutil create -volname Raiden -srcfolder "$work/disk" -fs HFS+ -format UDZO -ov "$dist/$name"; do
  tries=$((tries + 1))
  [ "$tries" -lt 3 ] || fail "could not make the disk image."
  sleep 5
done
codesign --force --timestamp --sign "$identity" "$dist/$name"
notarize "$dist/$name" "$@"
staple "$dist/$name"

notarized "$app" --type execute --verbose=2
notarized "$dist/$name" --type open --context context:primary-signature --verbose=2

(cd "$dist" && shasum -a 256 "$name" > "$name.sha256")
echo "Made $dist/$name, signed and notarized."
