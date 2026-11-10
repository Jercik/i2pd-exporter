#!/usr/bin/env bash
# Rendered by j4k-align from https://code.j4k.dev/j4k/align/src/branch/main/templates/repo/check-msg.sh.hbs
# Direct edits are drift and are overwritten on the next targeted `--fix`; edit the template.
set -euo pipefail
export LC_ALL=C.UTF-8

mode=commit
if [[ "$#" -eq 1 && "$1" == --title ]]; then
  mode=title
elif [[ "$#" -eq 1 && "$1" == --draft ]]; then
  mode=draft
elif [[ "$#" -ne 0 ]]; then
  printf '%s\n' 'usage: check-msg.sh [--title | --draft] < message' >&2
  exit 2
fi

message=$(git stripspace)
header=${message%%$'\n'*}
if [[ "$mode" != title ]]; then
  case "$header" in
    'Merge branch '*|'Merge remote-tracking branch '*|'Merge tag '*|'Merge pull request '*|'Merge commit '*|'Revert "'*|'Reapply "'*) exit 0 ;;
    'fixup! '*|'squash! '*|'amend! '*)
      if [[ "$mode" == draft ]]; then
        exit 0
      fi
      printf 'commit-msg: autosquash commit must be squashed before it lands (git rebase -i --autosquash <base>): %s\n' "$header" >&2
      exit 1
      ;;
  esac
fi

types='feat|fix|docs|refactor|test|chore|perf|build|ci|style|revert'
pattern="^($types)(\\([^)]+\\))?!?: .+"
if [[ ! "$header" =~ $pattern ]]; then
  printf "commit-msg: invalid header (expected 'type: subject' with type one of %s; the type may be followed by '(scope)', then by '!' for a breaking change): %s\n" "${types//|/, }" "$header" >&2
  exit 1
fi
if [[ "${#header}" -gt 100 ]]; then
  printf 'commit-msg: header has %s characters (maximum 100): %s\n' "${#header}" "$header" >&2
  exit 1
fi
