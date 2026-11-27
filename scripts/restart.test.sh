#!/bin/sh
set -u

here="$(cd "$(dirname "$0")" && pwd)"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
failures=0
cases=0

stub() {
  {
    echo '#!/bin/sh'
    echo '[ "${1:-}" = --raiden-stub ] && { echo stub; exit 0; }'
    cat
  } >"$world/bin/$1"
  chmod +x "$world/bin/$1"
}

stage() {
  cases=$((cases + 1))
  world="$root/$cases"
  mkdir -p "$world/bin"
  : >"$world/pids"
  : >"$world/zombies"
  : >"$world/calls"
  : >"$world/quitting"
  echo 0 >"$world/ticks"
  stub pgrep <<'EOF'
[ -n "${PGREP_FAILS:-}" ] && exit 3
[ "$*" = "-u $(id -u) -x Raiden" ] || { echo "pgrep $*" >>"$WORLD/calls"; exit 2; }
[ -s "$WORLD/pids" ] || exit 1
cat "$WORLD/pids"
EOF
  stub ps <<'EOF'
pid="$4"
if grep -qx "$pid" "$WORLD/zombies"; then echo Z; elif grep -qx "$pid" "$WORLD/pids"; then echo S; fi
EOF
  stub kill <<'EOF'
echo "kill $*" >>"$WORLD/calls"
if [ "$1" = -9 ]; then
  [ "${SURVIVES_KILL:-}" = 1 ] || { grep -vx "$2" "$WORLD/pids" >"$WORLD/next"; mv "$WORLD/next" "$WORLD/pids"; }
else
  [ "${IGNORES_QUIT:-}" = 1 ] || echo "$1 $(($(cat "$WORLD/ticks") + ${QUITS_AFTER:-0}))" >>"$WORLD/quitting"
fi
EOF
  stub open <<'EOF'
echo "open $*" >>"$WORLD/calls"
grep -qx 1111 "$WORLD/pids" && echo "opened beside the old copy" >>"$WORLD/calls"
tries=$(grep -c '^open ' "$WORLD/calls")
if [ "$tries" -le "${OPEN_FAILS:-0}" ]; then
  echo "_LSOpenURLsWithCompletionHandler() failed with error -600." >&2
  exit 1
fi
[ "$tries" -le "${OPEN_DOES_NOTHING:-0}" ] && exit 0
echo 4242 >>"$WORLD/pids"
[ -n "${QUITS_AFTER_TICKS:-}" ] && echo "4242 $(($(cat "$WORLD/ticks") + QUITS_AFTER_TICKS))" >>"$WORLD/quitting"
exit 0
EOF
  stub sleep <<'EOF'
ticks=$(($(cat "$WORLD/ticks") + 1))
echo "$ticks" >"$WORLD/ticks"
while read -r pid at; do
  if [ "$ticks" -ge "$at" ] && grep -qx "$pid" "$WORLD/pids"; then
    grep -vx "$pid" "$WORLD/pids" >"$WORLD/next"; mv "$WORLD/next" "$WORLD/pids"
    [ -n "${RESPAWNS_AS:-}" ] && [ "$pid" = 1111 ] && echo "$RESPAWNS_AS" >>"$WORLD/pids"
  fi
done <"$WORLD/quitting"
EOF
  for command in pgrep ps kill open sleep; do
    if [ "$(PATH="$world/bin:$PATH" WORLD="$world" env "$command" --raiden-stub 2>/dev/null)" != stub ]; then
      echo "not ok - the stand-in $command cannot run here, so the suite stops before touching Raiden"
      exit 1
    fi
  done
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
run env OPEN_DOES_NOTHING=2
check "$?:$(count '^open ')" "0:3" "tries again when open succeeds but nothing starts"

stage
run env OPEN_FAILS=99
check "$?:$(count '^open '):$(cat "$world/ticks")" "1:10:10" "gives up after 10 tries without waiting longer"
check "$(grep -c 'error -600' "$world/out")" "1" "says what Launch Services answered"

stage
echo 1111 >"$world/pids"
run env IGNORES_QUIT=1
check "$?:$(count 'kill -9 1111'):$(count 'beside'):$(cat "$world/pids")" "0:1:0:4242" \
  "forces a copy that ignores the request to quit"

stage
echo 1111 >"$world/pids"
run env IGNORES_QUIT=1 SURVIVES_KILL=1
check "$?:$(count '^open ')" "1:0" "never starts a second copy beside one that would not quit"

stage
echo 1111 >"$world/pids"
run env QUITS_AFTER=3 RESPAWNS_AS=5555
check "$?:$(count 'kill.*5555')" "0:0" "never kills a copy the login agent started meanwhile"

stage
run env QUITS_AFTER_TICKS=1
check "$?" "1" "fails when the new copy is seen running and then quits"

stage
echo 1111 >"$world/pids"
echo 1111 >"$world/zombies"
run env
check "$?:$(count '^kill'):$(count '^open ')" "0:0:1" "takes a copy that already exited for gone"

stage
echo 1111 >"$world/pids"
run env PGREP_FAILS=1
check "$?:$(count '^kill'):$(count '^open ')" "1:0:0" "stops when it cannot look for the running copy"

check "$(grep -c '^pgrep' "$root"/*/calls | awk -F: '{s+=$2} END {print s}')" "0" \
  "asks only about this user's Raiden, by its exact name"

exit "$failures"
