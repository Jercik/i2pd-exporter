#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C.UTF-8

mode=commit
if [[ "$#" -eq 1 && "$1" == --title ]]; then
  mode=title
elif [[ "$#" -ne 0 ]]; then
  printf '%s\n' 'usage: check-msg.sh [--title] < message' >&2
  exit 2
fi

message=$(git stripspace)
header=${message%%$'\n'*}
if [[ "$mode" == commit ]]; then
  case "$header" in
    'Merge branch '*|'Merge remote-tracking branch '*|'Merge tag '*|'Merge pull request '*|'Merge commit '*|'Revert "'*|'Reapply "'*|'fixup! '*|'squash! '*|'amend! '*) exit 0 ;;
  esac
fi

pattern='^(feat|fix|docs|refactor|test|chore|perf|build|ci|style|revert)(\([^)]+\))?!?: .+'
if [[ ! "$header" =~ $pattern ]]; then
  printf 'commit-msg: invalid header (expected a Conventional Commits header): %s\n' "$header" >&2
  exit 1
fi
if [[ "${#header}" -gt 100 ]]; then
  printf 'commit-msg: header has %s characters (maximum 100): %s\n' "${#header}" "$header" >&2
  exit 1
fi
