#!/usr/bin/env bash
# Builds a full templates dir: this repo's public templates/ (lib, schemas,
# samples, plain example .typ) with the private designs' same-named .typ files
# copied over the examples.
#
#   scripts/assemble-templates.sh <private-dir> <out-dir>
#
# <out-dir> may be this repo's own templates/ only in a throwaway CI checkout;
# locally, assemble outside the repo so private .typ files never show up as
# changes here.
set -euo pipefail

private="${1:?usage: assemble-templates.sh <private-dir> <out-dir>}"
out="${2:?usage: assemble-templates.sh <private-dir> <out-dir>}"
public="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/templates"

[ -d "$private/documents" ] || { echo "no documents/ in $private" >&2; exit 1; }

mkdir -p "$out"
if [ "$(cd "$out" && pwd)" != "$public" ]; then
  cp -R "$public/." "$out/"
fi
for kind in documents labels; do
  [ -d "$private/$kind" ] || continue
  mkdir -p "$out/$kind"
  cp "$private/$kind"/*.typ "$out/$kind/"
done
echo "assembled templates in $out"
