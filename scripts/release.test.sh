#!/bin/sh
set -eu

scripts="$(cd "$(dirname "$0")" && pwd)"
for variable in $(git rev-parse --local-env-vars); do
  unset "$variable"
done
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
failures=0

check() {
  if eval "$2"; then
    echo "ok: $1"
  else
    echo "FAILED: $1" >&2
    failures=$((failures + 1))
  fi
}

stand_in() {
  printf '#!/bin/sh\n%s\n' "$2" > "$root/bin/$1"
  chmod +x "$root/bin/$1"
}

mkdir -p "$root/bin"
stand_in security 'echo "$*" >> "$LOG"; printf "%s" "$IDENTITIES"'
stand_in tauri 'echo "tauri $* apple_id=${APPLE_ID:-}" >> "$LOG"
mkdir -p target/release/bundle/macos/Raiden.app/Contents/MacOS
echo built > target/release/bundle/macos/Raiden.app/Contents/MacOS/Raiden'
stand_in codesign 'echo "codesign $*" >> "$LOG"'
stand_in spctl 'echo "spctl $*" >> "$LOG"; printf "%s: accepted\\nsource=Notarized Developer ID\\n" "$5" >&2'
stand_in hdiutil 'echo "hdiutil $*" >> "$LOG"; for last; do :; done; echo disk > "$last"'
stand_in xcrun 'echo "xcrun $*" >> "$LOG"
case "$1 $2" in
  "notarytool submit")
    [ "${NOTARY_STATUS:-Accepted}" = Accepted ] || failed=1
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>id</key><string>submission-1</string><key>status</key><string>${NOTARY_STATUS:-Accepted}</string></dict></plist>
EOF
    ;;
  "notarytool log") echo "the notary log" ;;
  "notarytool history") [ -z "${NO_PROFILE:-}" ] || exit 1 ;;
  "stapler staple")
    if [ -n "${STAPLE_LATE:-}" ] && [ ! -f "$LOG.stapled" ]; then touch "$LOG.stapled"; exit 65; fi ;;
esac
exit "${failed:-0}"'

ONE='  1) ABC123 "Developer ID Application: Someone (TEAM1)"
     1 valid identities found'
TWO='  1) ABC123 "Developer ID Application: Someone (TEAM1)"
  2) DEF456 "Developer ID Application: Other (TEAM2)"
     2 valid identities found'

repo() {
  rm -rf "$root/repo"
  mkdir -p "$root/repo/scripts"
  cp "$scripts/release.sh" "$root/repo/scripts/"
  mkdir -p "$root/repo/ui/node_modules/.bin"
  ln -s "$root/bin/tauri" "$root/repo/ui/node_modules/.bin/tauri"
  printf '[package]\nname = "raiden"\nversion = "1.2.3"\n' > "$root/repo/Cargo.toml"
  printf 'target\nnode_modules\n' > "$root/repo/.gitignore"
  git -C "$root/repo" init -q
  git -C "$root/repo" add -A
  git -C "$root/repo" -c user.name=t -c user.email=t@t commit -qm init
}

release() {
  : > "$root/log"
  (cd "$root/repo" && env -u CARGO_TARGET_DIR -u NOTARY_KEY_PATH -u NOTARY_KEY_ID -u NOTARY_ISSUER -u RAIDEN_SIGNING_IDENTITY -u RAIDEN_NOTARY_PROFILE -u NOTARY_STATUS -u NO_PROFILE -u STAPLE_LATE \
    PATH="$root/bin:$PATH" LOG="$root/log" IDENTITIES="$IDENTITIES" "$@" \
    sh scripts/release.sh > "$root/out" 2>&1)
}

dmg="Raiden-$(uname -m).dmg"

repo
IDENTITIES="$ONE"
status=0
release APPLE_ID=someone@example.com || status=$?
check "a release succeeds" '[ $status -eq 0 ]'
check "it makes the disk image" '[ -f "$root/repo/target/dist/$dmg" ]'
check "it writes the image checksum" \
  '(cd "$root/repo/target/dist" && shasum -a 256 -c "$dmg.sha256" >/dev/null)'
check "it signs with the one Developer ID it found, with the hardened runtime and a timestamp" \
  'grep -q "codesign --force --options runtime --timestamp --sign ABC123 target/release/bundle/macos/Raiden.app" "$root/log"'
check "it builds without signing and without the Apple login Tauri would notarize with" \
  'grep -q "tauri build --bundles app --no-sign apple_id=$" "$root/log"'
check "it notarizes the app and the image with the saved keychain login" \
  '[ "$(grep -c "notarytool submit .* --keychain-profile raiden --wait" "$root/log")" -eq 2 ]'
check "it staples the app before putting it in the image" \
  '[ "$(grep -n "stapler staple target/release/bundle/macos/Raiden.app" "$root/log" | cut -d: -f1)" -lt "$(grep -n "hdiutil create" "$root/log" | cut -d: -f1)" ]'
check "it signs the image" 'grep -q "codesign --force --timestamp --sign ABC123 target/dist/$dmg" "$root/log"'
check "it staples the image" 'grep -q "stapler staple target/dist/$dmg" "$root/log"'
check "it checks the signed app" \
  'grep -q "codesign --verify --strict --verbose=2 target/release/bundle/macos/Raiden.app" "$root/log"'
check "it signs the image before sending it to Apple" \
  '[ "$(grep -n "codesign --force --timestamp --sign ABC123 target/dist/" "$root/log" | cut -d: -f1)" -lt "$(grep -n "notarytool submit target/dist/" "$root/log" | cut -d: -f1)" ]'
check "it asks Gatekeeper about the app" \
  'grep -q "spctl --assess --type execute --verbose=2 target/release/bundle/macos/Raiden.app" "$root/log"'
check "it asks Gatekeeper about the image" \
  'grep -q "spctl --assess --type open --context context:primary-signature --verbose=2 target/dist/$dmg" "$root/log"'

repo
status=0
release NOTARY_KEY_PATH=/k.p8 NOTARY_KEY_ID=KEY NOTARY_ISSUER=ISSUER || status=$?
check "on CI it notarizes with an API key" \
  '[ "$(grep -c "notarytool submit .* --key /k.p8 --key-id KEY --issuer ISSUER --wait" "$root/log")" -eq 2 ]'

repo
status=0
release NOTARY_KEY_PATH=/k.p8 || status=$?
check "an API key without its id and issuer stops the release" \
  '[ $status -ne 0 ] && grep -q "NOTARY_KEY_ID and NOTARY_ISSUER" "$root/out"'

repo
echo change >> "$root/repo/Cargo.toml"
status=0
release || status=$?
check "uncommitted changes stop the release before building" \
  '[ $status -ne 0 ] && ! grep -q tauri "$root/log" && grep -q "commit or remove" "$root/out"'

repo
IDENTITIES='     0 valid identities found'
status=0
release || status=$?
check "no Developer ID stops the release and says how to make one" \
  '[ $status -ne 0 ] && ! grep -q tauri "$root/log" && grep -q Xcode "$root/out"'

IDENTITIES="$TWO"
status=0
release || status=$?
check "two Developer IDs stop the release and ask which" \
  '[ $status -ne 0 ] && grep -q RAIDEN_SIGNING_IDENTITY "$root/out"'
status=0
release RAIDEN_SIGNING_IDENTITY=DEF456 || status=$?
check "the chosen Developer ID signs" 'grep -q "codesign .* --sign DEF456 " "$root/log"'

IDENTITIES="$ONE"
stand_in hdiutil 'echo "hdiutil $*" >> "$LOG"
if [ ! -f "$LOG.busy" ]; then touch "$LOG.busy"; echo "Resource busy" >&2; exit 1; fi
for last; do :; done; echo disk > "$last"'
stand_in sleep 'echo "sleep $*" >> "$LOG"'
repo
rm -f "$root/log.busy"
status=0
release || status=$?
check "a busy disk is tried again" \
  '[ $status -eq 0 ] && [ "$(grep -c "^hdiutil create" "$root/log")" -eq 2 ]'
stand_in hdiutil 'echo "hdiutil $*" >> "$LOG"; echo "Resource busy" >&2; exit 1'
status=0
release || status=$?
check "a disk that stays busy stops the release after 3 tries" \
  '[ $status -ne 0 ] && [ "$(grep -c "^hdiutil create" "$root/log")" -eq 3 ]'
stand_in hdiutil 'echo "hdiutil $*" >> "$LOG"; for last; do :; done; echo disk > "$last"'

stand_in spctl 'echo "spctl $*" >> "$LOG"; echo "rejected" >&2; exit 3'
status=0
release || status=$?
check "a Gatekeeper refusal stops the release before the checksum" \
  '[ $status -ne 0 ] && [ ! -f "$root/repo/target/dist/$dmg.sha256" ]'
stand_in spctl 'echo "spctl $*" >> "$LOG"; printf "accepted\\nsource=Developer ID\\n" >&2'
status=0
release || status=$?
check "a signed but not notarized download stops the release" \
  '[ $status -ne 0 ] && grep -q "not see .* as notarized" "$root/out" && [ ! -f "$root/repo/target/dist/$dmg.sha256" ]'
stand_in spctl 'echo "spctl $*" >> "$LOG"; printf "%s: accepted\\nsource=Notarized Developer ID\\n" "$5" >&2'

IDENTITIES='  1) ABC123 "Developer ID Application: Someone (TEAM1)"
  2) ABC123 "Developer ID Application: Someone (TEAM1)"'
repo
status=0
release || status=$?
check "the same certificate in two keychains counts once" \
  '[ $status -eq 0 ] && grep -q "codesign .* --sign ABC123 " "$root/log"'
IDENTITIES="$ONE"

rm -f "$root/log.stapled"
status=0
release STAPLE_LATE=1 || status=$?
check "a ticket that is not ready yet is stapled on a later try" \
  '[ $status -eq 0 ] && grep -q "sleep 15" "$root/log"'
check "it validates both staples" '[ "$(grep -c "stapler validate" "$root/log")" -eq 2 ]'

status=0
release NO_PROFILE=1 || status=$?
check "a missing notary login stops the release before building" \
  '[ $status -ne 0 ] && ! grep -q "tauri build" "$root/log" && grep -q "store-credentials raiden" "$root/out"'

status=0
release NOTARY_STATUS=Invalid || status=$?
check "a rejection stops the release and shows Apple's log" \
  '[ $status -ne 0 ] && grep -q "the notary log" "$root/out" && grep -q "Invalid" "$root/out"'
check "a rejected app is never stapled" '! grep -q "stapler" "$root/log"'

repo
IDENTITIES='     0 valid identities found'
release_stage() {
  : > "$root/log"
  (cd "$root/repo" && env -u CARGO_TARGET_DIR -u NOTARY_KEY_PATH -u NOTARY_KEY_ID -u NOTARY_ISSUER -u RAIDEN_SIGNING_IDENTITY -u RAIDEN_NOTARY_PROFILE -u NOTARY_STATUS -u NO_PROFILE -u STAPLE_LATE \
    PATH="$root/bin:$PATH" LOG="$root/log" IDENTITIES="$IDENTITIES" \
    sh scripts/release.sh "$1" > "$root/out" 2>&1)
}
status=0
release_stage build || status=$?
check "building alone needs no certificate and signs nothing" \
  '[ $status -eq 0 ] && grep -q "tauri build" "$root/log" && ! grep -q "codesign\|xcrun\|security" "$root/log"'

IDENTITIES="$ONE"
echo change >> "$root/repo/Cargo.toml"
status=0
release_stage sign || status=$?
check "signing alone builds nothing" \
  '[ $status -eq 0 ] && ! grep -q tauri "$root/log" && grep -q "stapler staple target/dist/$dmg" "$root/log"'

repo
status=0
release_stage sign || status=$?
check "signing before building says to build first" \
  '[ $status -ne 0 ] && grep -q "Build it first" "$root/out"'

notes() {
  rm -rf "$root/notes"
  git init -q "$root/notes"
  for subject in "$@"; do
    git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "$subject"
  done
}

notes "feat(ui): first feature" "chore: old chore"
git -C "$root/notes" tag v0.1.0
git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "feat(ui): Tab asks AI"
git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "fix: the window stays put"
git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "feat!: settings move"
git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "docs: explain"
git -C "$root/notes" tag v0.2.0
written="$(cd "$root/notes" && GITHUB_REPOSITORY=owner/repo sh "$scripts/release-notes.sh" v0.2.0)"
expected='## New

- settings move
- Tab asks AI

## Fixed

- the window stays put

Every change: https://github.com/owner/repo/compare/v0.1.0...v0.2.0'
check "the notes list what is new and fixed since the last release" '[ "$written" = "$expected" ]'

git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "fix: beta fix"
git -C "$root/notes" tag v0.3.0-beta.1
git -C "$root/notes" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "feat: after the beta"
git -C "$root/notes" tag v0.3.0
written="$(cd "$root/notes" && GITHUB_REPOSITORY=owner/repo sh "$scripts/release-notes.sh" v0.3.0)"
expected='## New

- after the beta

## Fixed

- beta fix

Every change: https://github.com/owner/repo/compare/v0.2.0...v0.3.0'
check "a release lists everything since the last release, betas included" '[ "$written" = "$expected" ]'
written="$(cd "$root/notes" && GITHUB_REPOSITORY=owner/repo sh "$scripts/release-notes.sh" v0.3.0-beta.1)"
check "a beta lists what changed since the tag before it" \
  'printf "%s" "$written" | grep -q "compare/v0.2.0...v0.3.0-beta.1"'

written="$(cd "$root/notes" && sh "$scripts/release-notes.sh" v0.1.0)"
expected='## New

- first feature'
check "the first release lists everything before it" '[ "$written" = "$expected" ]'

if [ "$failures" -gt 0 ]; then
  echo "$failures release checks failed." >&2
  exit 1
fi
