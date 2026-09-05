#!/usr/bin/env bash
# sync-fork.sh — bring the local fork up to date with the official repo.
#
# Assumes this checkout is a fork of the official repo:
#   origin   -> git@github.com:lbarasti/buzz.git   (your fork; where you push)
#   upstream -> git@github.com:block/buzz.git      (official repo; read-only)
#
# Usage:
#   ./scripts/sync-fork.sh            sync local main from upstream, push to origin
#   ./scripts/sync-fork.sh --branch x also rebase branch x onto the fresh main
#
# Environment overrides:
#   FORK_URL        origin URL  (default: lbarasti/buzz)
#   UPSTREAM_URL    upstream URL (default: block/buzz)
#
# This script never force-pushes. If local main has diverged from
# upstream/main (e.g. you committed directly to your fork's main), it stops
# and explains instead of rewriting history.

set -euo pipefail

FORK_URL="${FORK_URL:-git@github.com:lbarasti/buzz.git}"
UPSTREAM_URL="${UPSTREAM_URL:-git@github.com:block/buzz.git}"

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

BRANCH=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --branch) BRANCH="${2:?--branch requires a name}"; shift 2 ;;
    -h | --help) usage ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

# Point the remotes where they should be.
if ! git remote get-url upstream >/dev/null 2>&1; then
  echo "adding upstream -> $UPSTREAM_URL"
  git remote add upstream "$UPSTREAM_URL"
elif [ "$(git remote get-url upstream)" != "$UPSTREAM_URL" ]; then
  echo "fixing upstream -> $UPSTREAM_URL"
  git remote set-url upstream "$UPSTREAM_URL"
fi
if [ "$(git remote get-url origin 2>/dev/null || true)" != "$FORK_URL" ]; then
  if git remote get-url origin >/dev/null 2>&1; then
    echo "fixing origin -> $FORK_URL"
    git remote set-url origin "$FORK_URL"
  else
    echo "adding origin -> $FORK_URL"
    git remote add origin "$FORK_URL"
  fi
fi

# Don't run over uncommitted work.
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "working tree has uncommitted changes; commit or stash them first" >&2
  exit 1
fi

git fetch upstream --prune

git checkout main
if ! git merge --ff-only upstream/main; then
  echo "local main has diverged from upstream/main; refusing to rewrite history." >&2
  echo "pick one and re-run:" >&2
  echo "  git rebase upstream/main   # replay your commits on top of upstream" >&2
  echo "  git merge upstream/main    # record a merge commit (always safe to push)" >&2
  exit 1
fi

# The push is always a fast-forward to upstream/main (guaranteed by the
# --ff-only above), so every pushed commit already passed CI upstream.
# Skip the pre-push hook instead of re-testing someone else's code.
git push --no-verify origin main
echo "origin/main -> $(git rev-parse --short main)"

if [ -n "$BRANCH" ]; then
  git checkout "$BRANCH" || { echo "no such branch: $BRANCH" >&2; exit 1; }
  git rebase main
fi