#!/usr/bin/env bash
# Publish screenshots for an issue or pull request and print the Markdown that
# shows them. `gh` cannot upload attachments, so the images go to the
# repository's `issue-media` branch (created on first use, with no history
# shared with main) under a folder named after the issue or PR number.
#
# The repository is public and a pushed image stays in that branch's history.
# Look at every image before running this (see AGENTS.md, "Visual issues").
#
# Usage: scripts/issue-screenshot.sh <issue-or-pr-number> <image>...
# Env:   HARNESS_MEDIA_REPO (default: the current gh repository),
#        HARNESS_MEDIA_DRY_RUN=1 prints what would be uploaded.

set -euo pipefail

if [ "$#" -lt 2 ] || ! [[ "$1" =~ ^[0-9]+$ ]]; then
  echo "usage: $0 <issue-or-pr-number> <image>..." >&2
  exit 2
fi
NUMBER="$1"
shift
BRANCH="issue-media"
REPO="${HARNESS_MEDIA_REPO:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
DRY_RUN="${HARNESS_MEDIA_DRY_RUN:-}"

for image in "$@"; do
  case "$(printf '%s' "${image##*.}" | tr '[:upper:]' '[:lower:]')" in
    png | jpg | jpeg | gif | webp) ;;
    *) echo "not an image: $image" >&2; exit 2 ;;
  esac
  [ -f "$image" ] || { echo "no such file: $image" >&2; exit 2; }
  if [ "$(wc -c <"$image")" -gt 5000000 ]; then
    echo "over 5 MB, crop or scale it first: $image" >&2
    exit 2
  fi
done

if [ -z "$DRY_RUN" ] && ! gh api "repos/$REPO/branches/$BRANCH" >/dev/null 2>&1; then
  readme="Screenshots for issues and pull requests, one folder per number. Not part of the app."
  tree="$(jq -n --arg c "$readme" '{tree: [{path: "README.md", mode: "100644", type: "blob", content: $c}]}' |
    gh api "repos/$REPO/git/trees" --input - -q .sha)"
  commit="$(gh api "repos/$REPO/git/commits" -f message="Start the issue media branch" -f tree="$tree" -q .sha)"
  gh api "repos/$REPO/git/refs" -f ref="refs/heads/$BRANCH" -f sha="$commit" >/dev/null
fi

stamp="$(date -u +%Y%m%d-%H%M%S)"
for image in "$@"; do
  name="$(basename "$image" | tr -c 'A-Za-z0-9._\n-' '-')"
  path="$NUMBER/$stamp-$name"
  if [ -n "$DRY_RUN" ]; then
    echo "would upload $image to $REPO@$BRANCH:$path" >&2
  else
    base64 <"$image" | tr -d '\n' |
      jq -Rs --arg m "Screenshot for #$NUMBER" --arg b "$BRANCH" '{message: $m, branch: $b, content: .}' |
      gh api -X PUT "repos/$REPO/contents/$path" --input - >/dev/null
  fi
  echo "![${name%.*}](https://raw.githubusercontent.com/$REPO/$BRANCH/$path)"
done
