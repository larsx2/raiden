#!/bin/sh
set -eu

tag="${1:?usage: release-notes.sh <tag>}"
case "$tag" in
  *-*) set -- ;;
  *) set -- --exclude 'v*-*' ;;
esac
previous="$(git describe --tags --abbrev=0 --match 'v*' "$@" "$tag^" 2>/dev/null || true)"
if [ -n "$previous" ]; then
  range="$previous..$tag"
else
  range="$tag"
fi

section() {
  lines="$(git log --no-merges --format=%s "$range" \
    | sed -n "s/^$2\(([^)]*)\)\{0,1\}!\{0,1\}: \(.*\)/- \2/p")"
  if [ -n "$lines" ]; then
    printf '## %s\n\n%s\n\n' "$1" "$lines"
  fi
}

section New feat
section Fixed fix
if [ -n "$previous" ]; then
  echo "Every change: https://github.com/${GITHUB_REPOSITORY:-larsx2/raiden}/compare/$previous...$tag"
fi
