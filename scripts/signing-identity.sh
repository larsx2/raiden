#!/bin/sh
set -eu

name="Raiden Local"
folder="$HOME/Library/Application Support/raiden"
keychain="$HOME/Library/Keychains/raiden-signing.keychain-db"
secret="$folder/signing-keychain-password"

add_to_search_list() {
  current="$(security list-keychains -d user | sed -e 's/^ *"//' -e 's/"$//')"
  if [ -z "$current" ]; then
    security list-keychains -d user -s "$HOME/Library/Keychains/login.keychain-db" "$keychain"
    return
  fi
  case "$current" in
    *"$keychain"*) ;;
    *) eval "set -- $(printf '%s\n' "$current" | sed 's/.*/"&"/' | tr '\n' ' ')"
       security list-keychains -d user -s "$@" "$keychain" ;;
  esac
}

if [ -f "$keychain" ] && [ -f "$secret" ] \
  && security unlock-keychain -p "$(cat "$secret")" "$keychain" 2>/dev/null \
  && security find-identity -p codesigning "$keychain" | grep -q "\"$name\""; then
  add_to_search_list
  exit 0
fi

mkdir -p "$folder"
umask 077
password="$(openssl rand -hex 24)"
printf '%s' "$password" > "$secret"
security delete-keychain "$keychain" 2>/dev/null || rm -f "$keychain"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings "$keychain"
security unlock-keychain -p "$password" "$keychain"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
transfer="$(openssl rand -hex 16)"

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -subj "/CN=$name" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -addext "basicConstraints=critical,CA:false" \
  -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null

openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" -name "$name" \
  -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 \
  -passout "pass:$transfer" -out "$work/identity.p12"

security import "$work/identity.p12" -k "$keychain" -P "$transfer" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
add_to_search_list

echo "Created the \"$name\" signing identity in its own keychain, unlocked without your password."
