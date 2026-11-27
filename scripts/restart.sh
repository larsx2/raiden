#!/bin/sh
set -eu

app="$1"
me="$(id -u)"

running() {
  for pid in $(pgrep -u "$me" -x Raiden); do
    case "$(ps -o stat= -p "$pid" 2>/dev/null)" in
      "" | Z*) ;;
      *) return 0 ;;
    esac
  done
  return 1
}

quit_within() {
  for _ in $(seq "$1"); do
    running || return 0
    sleep 0.1
  done
  ! running
}

if running; then
  pkill -u "$me" -x Raiden || true
  if ! quit_within 50; then
    pkill -9 -u "$me" -x Raiden || true
    if ! quit_within 20; then
      echo "The running Raiden would not quit, so the new one was not started." >&2
      exit 1
    fi
  fi
fi

error=""
for _ in $(seq 10); do
  if error="$(open "$app" 2>&1)"; then
    error=""
    break
  fi
  sleep 0.5
done

for _ in $(seq 100); do
  running && break
  sleep 0.1
done
sleep 1
if ! running; then
  echo "Raiden is installed but did not start${error:+: $error}. Open it from ~/Applications." >&2
  exit 1
fi
