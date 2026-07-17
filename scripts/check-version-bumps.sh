#!/usr/bin/env bash
#
# CI guard: every component whose code or WIT changed vs the base branch must
# also have its WIT package version bumped.
#
# For each component we compare the version in its `package <ns>:<name>@X.Y.Z;`
# declaration against the same file on the base branch. A component is
# considered "changed" when any of these (tracked) files differ from the base:
#   - <component>/src/**, <component>/build.rs, <component>/Cargo.toml,
#     <component>/wit/** (excluding wit/deps/**)
#
# A component root is the parent of its wit/ directory, so this covers both
# functions (functions/<name>/<version>/) and components (components/<name>/).
#
# Generated/tooling files never trigger the requirement: wit/deps/ (gitignored),
# target/ (gitignored), wkg.lock, .wash/, tests/, docs, Justfile, etc.
#
# Usage:
#   GITHUB_BASE_REF=main ./scripts/check-version-bumps.sh
#   ./scripts/check-version-bumps.sh <base-ref-or-sha>
set -uo pipefail

BASE_REF="${1:-${GITHUB_BASE_REF:-main}}"

# Make the base ref available (no-op if already fetched, e.g. local dev).
git fetch -q origin "$BASE_REF" 2>/dev/null || true

if git rev-parse --verify -q "origin/${BASE_REF}^{commit}" >/dev/null; then
  BASE="origin/${BASE_REF}"
elif git rev-parse --verify -q "${BASE_REF}^{commit}" >/dev/null; then
  BASE="${BASE_REF}"
else
  echo "::error::base ref '${BASE_REF}' not found (tried origin/${BASE_REF} and ${BASE_REF})"
  exit 1
fi

echo "Comparing against base: ${BASE}"
changed="$(git diff --name-only "${BASE}...HEAD")"
if [ -z "$changed" ]; then
  echo "No changes vs base. Nothing to check."
  exit 0
fi

# Extract the version that follows '@' in the first `package ...;` line (reads stdin).
version_of() {
  grep -m1 -E '^package ' | sed -nE 's/^package[^@]*@([^;[:space:]]+).*/\1/p'
}

errors=()

# require_bump <label> <version-file> <include-ERE> [<exclude-ERE>]
require_bump() {
  local label="$1" vfile="$2" include="$3" exclude="${4:-}"
  local hits
  hits="$(printf '%s\n' "$changed" | grep -E "$include" || true)"
  [ -n "$exclude" ] && hits="$(printf '%s\n' "$hits" | grep -vE "$exclude" || true)"
  hits="$(printf '%s\n' "$hits" | grep -v '^[[:space:]]*$' || true)"
  [ -z "$hits" ] && return 0   # nothing relevant to this component changed

  local cur base
  cur="$(version_of < "$vfile" 2>/dev/null)"
  base="$(git show "${BASE}:${vfile}" 2>/dev/null | version_of)"

  if [ -z "$base" ]; then
    echo "🆕 ${label}: new component (no version on base) — OK"
    return 0
  fi
  if [ -z "$cur" ]; then
    errors+=("${label} — changed but no '@version' found in ${vfile}")
    echo "❌ ${label}: changed but version is missing/unparseable in ${vfile}"
    return 0
  fi
  if [ "$cur" = "$base" ]; then
    errors+=("${label} — changed but version NOT bumped (still @${cur}); bump the version in ${vfile}")
    echo "❌ ${label}: changed, version still @${cur}"
  else
    echo "✅ ${label}: changed, version @${base} → @${cur}"
  fi
}

# Components: functions/**/wit/world.wit and components/**/wit/world.wit
# (a component root is the parent of its wit/ directory).
while IFS= read -r world; do
  [ -z "$world" ] && continue
  dir="$(dirname "$(dirname "$world")")"
  require_bump "$dir" "$world" \
    "^${dir}/(src/|build\.rs$|Cargo\.toml$|wit/)" \
    "^${dir}/wit/deps/"
done < <(find functions components -name world.wit -not -path '*/wit/deps/*' 2>/dev/null | sort)

echo
if [ "${#errors[@]}" -gt 0 ]; then
  echo "::error::Version bump required — the following components changed without a version bump:"
  for e in "${errors[@]}"; do echo "  • ${e}"; done
  exit 1
fi
echo "All changed components have a version bump. ✅"
