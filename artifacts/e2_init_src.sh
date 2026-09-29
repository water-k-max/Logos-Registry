#!/usr/bin/env bash
set -uo pipefail
cd "$HOME/.cargo/git/checkouts/spel-411f019390120158" || exit 1
echo "=== spel checkouts"; ls
D=$(ls -d */ | head -1)
echo "=== grep init constraint implementation"
grep -rn --include=*.rs -E 'fn .*init|AlreadyInitialized|already initialized|IsInitialized|non_zero|must not exist|DataAlreadyExists|AccountAlready' . 2>/dev/null | grep -i init | head -25
