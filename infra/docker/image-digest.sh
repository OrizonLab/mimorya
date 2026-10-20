#!/usr/bin/env bash
# Prints the digest a published image reference names (ADR-051), or nothing
# when the registry has no such image. Any other answer (network, rate
# limit, authorization) fails: an image is never taken for absent because
# the registry did not say so.
#   infra/docker/image-digest.sh ghcr.io/orizonlab/mimorya-api:sha-…
set -euo pipefail

ref=$1
errors=$(mktemp)
trap 'rm -f "$errors"' EXIT

if manifest=$(docker buildx imagetools inspect "$ref" --format '{{json .Manifest}}' 2>"$errors"); then
  jq -er .digest <<<"$manifest"
elif grep -qiE ': not found$|manifest unknown|name unknown' "$errors"; then
  :
else
  cat "$errors" >&2
  exit 1
fi
