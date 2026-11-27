#!/bin/sh
set -u

here="$(cd "$(dirname "$0")" && pwd)"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
failures=0
cases=0

stage() {
  cases=$((cases + 1))
  world="$root/$cases"
  mkdir -p "$world/bin"
  : >"$world/pids"
  : >"$world/zombies"
  : >"$world/calls"
  echo 0 >"$world/polls"
  cat >"$world/bin/pgrep" <<'EOF'
#!/bin/sh
[ "$*" = "-u $(id -u) -x Raiden" ] || { echo "pgrep $*" >>"$WORLD/calls"; exit 2; }
polls=$(($(cat "$WORLD/polls") + 1))
echo "$polls" >"$WORLD/polls"
[ -s "$WORLD/pids" ] && cat "$WORLD/pids"
EOF
  cat >"$world/bin/ps" <<'EOF'
#!/bin/sh
pid="$4"
if grep -qx "$pid" "$WORLD/zombies"; then echo Z; elif grep -qx "$pid" "$WORLD/pids"; then echo S; fi
EOF
  cat >"$world/bin/pkill" <<'EOF'
#!/bin/sh
echo "pkill $*" >>"$WORLD/calls"
case "$*" in
  *-9*) [ "${SURVIVES_KILL:-}" = 1 ] || : >"$WORLD/pids" ;;
  *) [ "${IGNORES_QUIT:-}" = 1 ] || echo "$(cat "$WORLD/polls")" >"$WORLD/quit-asked" ;;
esac
EOF
  cat >"$world/bin/open" <<'EOF'
#!/bin/sh
echo "open $*" >>"$WORLD/calls"
grep -qx 1111 "$WORLD/pids" && echo "opened beside the old copy" >>"$WORLD/calls"
tries=$(grep -c '^open' "$WORLD/calls")
if [ "$tries" -le "${OPEN_FAILS:-0}" ]; then
  echo "_LSOpenURLsWithCompletionHandler() failed with error -600." >&2
  exit 1
fi
echo 4242 >"$WORLD/pids"
if [ -n "${QUITS_AFTER_POLLS:-}" ]; then
  echo $(($(cat "$WORLD/polls") + QUITS_AFTER_POLLS)) >"$WORLD/new-quits-at"
fi
EOF
  cat >"$world/bin/sleep" <<'EOF'
#!/bin/sh
polls=$(cat "$WORLD/polls")
if [ -f "$WORLD/quit-asked" ] && [ "$polls" -ge $(($(cat "$WORLD/quit-asked") + ${QUITS_AFTER:-0})) ]; then
  grep -qx 1111 "$WORLD/pids" && : >"$WORLD/pids"
  rm -f "$WORLD/quit-asked"
fi
if [ -f "$WORLD/new-quits-at" ] && [ "$polls" -ge "$(cat "$WORLD/new-quits-at")" ]; then
  : >"$WORLD/pids"
fi
EOF
  chmod +x "$world/bin/"*
}

run() {
  PATH="$world/bin:$PATH" WORLD="$world" "$@" sh "$here/restart.sh" /Applications/Raiden.app \
    >"$world/out" 2>&1
}

check() {
  if [ "$1" != "$2" ]; then
    echo "not ok - $3: expected $2, got $1"
    sed 's/^/    /' "$world/out" "$world/calls"
    failures=$((failures + 1))
  else
    echo "ok - $3"
  fi
}

count() {
  grep -c "$1" "$world/calls"
}

stage
echo 1111 >"$world/pids"
run env QUITS_AFTER=6
check "$?:$(count '^open '):$(count 'beside'):$(cat "$world/pids")" "0:1:0:4242" \
  "waits for the old copy to quit before starting the new one"

stage
run env OPEN_FAILS=3
check "$?:$(count '^open ')" "0:4" "tries again when Launch Services refuses the bundle it just replaced"

stage
run env OPEN_FAILS=99
check "$?:$(count '^open ')" "1:10" "gives up after 10 tries"
check "$(grep -c 'error -600' "$world/out")" "1" "says what Launch Services answered"

stage
echo 1111 >"$world/pids"
run env IGNORES_QUIT=1
check "$?:$(count 'pkill -9'):$(count 'beside'):$(cat "$world/pids")" "0:1:0:4242" \
  "forces a copy that ignores the request to quit"

stage
echo 1111 >"$world/pids"
run env IGNORES_QUIT=1 SURVIVES_KILL=1
check "$?:$(count '^open ')" "1:0" "never starts a second copy beside one that would not quit"

stage
run env QUITS_AFTER_POLLS=1
check "$?" "1" "fails when the new copy is seen running and then quits"

stage
echo 1111 >"$world/pids"
echo 1111 >"$world/zombies"
run env
check "$?:$(count '^pkill'):$(count '^open ')" "0:0:1" "takes a copy that already exited for gone"

check "$(grep -c '^pgrep' "$root"/*/calls | awk -F: '{s+=$2} END {print s}')" "0" \
  "asks only about this user's Raiden, by its exact name"

exit "$failures"
