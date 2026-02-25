#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  bash scripts/flows_coverage.sh [--file README.md] [--section "Flow checklist (E2E use-cases)"] [--min 80]

Prints flow coverage from a GitHub-flavored Markdown checklist section:
  Flows covered: <checked>/<total> (<percent>%)

Options:
  --file <path>      Markdown file to scan (default: README.md)
  --section <title>  Section title to scan (default: Flow checklist (E2E use-cases))
  --min <percent>    If set, exits non-zero when coverage is below this threshold
EOF
}

file="README.md"
section="Flow checklist (E2E use-cases)"
min=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --file)
      file="${2:-}"; shift 2 ;;
    --section)
      section="${2:-}"; shift 2 ;;
    --min)
      min="${2:-}"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$file" ]]; then
  echo "File not found: $file" >&2
  exit 2
fi

# Extract checklist lines within the target section (until next '## ' heading).
# Count total checklist items and checked ones.
read -r total checked percent <<<"$(awk -v section="$section" '
  function ltrim(s) { sub(/^[[:space:]]+/, "", s); return s }
  function rtrim(s) { sub(/[[:space:]]+$/, "", s); return s }
  function trim(s) { return rtrim(ltrim(s)) }

  BEGIN { in_section=0; total=0; checked=0 }

  {
    line=$0
    if (!in_section) {
      if (trim(line) == "## " section) { in_section=1; next }
      next
    }

    # Stop at the next section heading.
    if (match(line, /^##[[:space:]]+/)) { in_section=0; next }

    if (match(line, /^- \[[ xX]\][[:space:]]+/)) {
      total++
      if (match(line, /^- \[[xX]\][[:space:]]+/)) checked++
    }
  }

  END {
    if (total == 0) {
      printf("%d %d %s\n", 0, 0, "0.0")
      exit 0
    }
    pct = (checked * 100.0) / total
    printf("%d %d %.1f\n", total, checked, pct)
  }
' "$file")"

if [[ "$total" -eq 0 ]]; then
  echo "No checklist items found under section: $section" >&2
  exit 2
fi

echo "Flows covered: ${checked}/${total} (${percent}%)"

if [[ -n "$min" ]]; then
  # Compare as integers by truncating percent.
  # (We still print the 1-decimal percent for humans.)
  percent_int="${percent%.*}"
  if [[ "$percent_int" -lt "$min" ]]; then
    echo "FAIL: flow coverage ${percent}% is below minimum ${min}%" >&2
    exit 1
  fi
fi
