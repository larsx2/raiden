#!/bin/sh
set -eu

app="${1:?usage: restart.sh /path/to/Raiden.app}"
me="$(id -u)"

raiden_pids() {
  found="$(pgrep -u "$me" -x Raiden)" && {
    echo "$found"
    return 0
  }
  [ $? -eq 1 ] && return 0
  echo "Raiden could not look for its running copy." >&2
  exit 1
}

alive() {
  case "$(ps -o stat= -p "$1" 2>/dev/null)" in
    "" | Z*) return 1 ;;
    *) return 0 ;;
  esac
}

any_alive() {
  for pid in $1; do
    alive "$pid" && return 0
  done
  return 1
}

gone_within() {
  for _ in $(seq "$2"); do
    any_alive "$1" || return 0
    sleep 0.1
  done
  ! any_alive "$1"
}

old="$(raiden_pids)"
if any_alive "$old"; then
  for pid in $old; do env kill "$pid" 2>/dev/null || true; done
  if ! gone_within "$old" 50; then
    for pid in $old; do env kill -9 "$pid" 2>/dev/null || true; done
    if ! gone_within "$old" 20; then
      echo "The running Raiden would not quit, so the new one was not started." >&2
      exit 1
    fi
  fi
fi

new_pid() {
  for pid in $(raiden_pids); do
    case " $(echo $old) " in
      *" $pid "*) ;;
      *) alive "$pid" && echo "$pid" && return 0 ;;
    esac
  done
  return 1
}

started=""
error=""
for _ in $(seq 10); do
  if error="$(open "$app" 2>&1)"; then
    error=""
    for _ in $(seq 20); do
      started="$(new_pid)" && break
      sleep 0.1
    done
    [ -n "$started" ] && break
  fi
  sleep 0.5
done

if [ -z "$started" ]; then
  echo "Raiden is installed but did not start${error:+: $error}. Open it from ~/Applications." >&2
  exit 1
fi
sleep 1
if ! alive "$started"; then
  echo "Raiden is installed but quit right after starting. Open it from ~/Applications." >&2
  exit 1
fi
