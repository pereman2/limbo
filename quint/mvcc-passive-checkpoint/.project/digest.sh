#!/usr/bin/env bash
# Builds DIGEST.md from GOAL.md, LEDGER.md, DECISIONS.md and RESUME.md.
# Run it from any directory. Keeps the result under 5 KB.
set -euo pipefail
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="$dir/DIGEST.md"
max=5000

{
  echo "# Digest ($(date +%F))"
  echo
  echo "## Goal"
  sed -n '/^## G/,$p' "$dir/GOAL.md" | grep -E '^## G' || true
  echo
  echo "## Open ledger rows (id | status | what | owner)"
  awk -F'|' '/^\| L[0-9]/ {
    id=$2; what=$3; owner=$4; status=$6
    gsub(/^ +| +$/, "", id); gsub(/^ +| +$/, "", status)
    gsub(/^ +| +$/, "", owner)
    if (status ~ /^done/) next
    w=substr(what, 2, 150); gsub(/^ +/, "", w)
    printf "- %s | %s | %s | %s\n", id, status, w, substr(owner, 1, 60)
  }' "$dir/LEDGER.md"
  echo
  echo "## Last decisions"
  grep -E '^- 20[0-9]{2}-' "$dir/DECISIONS.md" | tail -6 | cut -c1-220
  echo
  echo "## Resume"
  sed -n '/^## Open items/,/^## In-flight/p' "$dir/RESUME.md" | grep -vE '^## In-flight' | cut -c1-260
  sed -n '/^## Next three actions/,$p' "$dir/RESUME.md" | cut -c1-260
} > "$out.tmp"

size=$(wc -c < "$out.tmp")
if [ "$size" -gt "$max" ]; then
  head -c "$max" "$out.tmp" > "$out"
  printf '\n(cut at %s bytes)\n' "$max" >> "$out"
else
  cp "$out.tmp" "$out"
fi
rm -f "$out.tmp"
echo "DIGEST.md: $(wc -c < "$out") bytes"
