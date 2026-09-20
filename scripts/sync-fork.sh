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
# Fork commits on local main are preserved: when main has diverged, its local
# commits are rebased onto upstream/main instead of the sync being refused.
# Conflicts are resolved automatically in favour of the replayed local commit
# (`-X theirs` during a rebase, where "theirs" is the commit being replayed);
# non-conflicting upstream changes still apply. Previously recorded conflict
# resolutions are replayed via `git rerere`.
#
# Because the rebase rewrites the local commits, origin/main is updated with a
# `--force-with-lease` push. If the fork advanced since this checkout last
# fetched origin, the lease fails rather than overwriting those commits; fetch
# and re-run in that case. The push skips the pre-push hook to stay
# non-interactive; run `just ci` for your local commits.

set -euo pipefail

FORK_URL="${FORK_URL:-git@github.com:lbarasti/buzz.git}"
UPSTREAM_URL="${UPSTREAM_URL:-git@github.com:block/buzz.git}"

usage() {
  sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'
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

# Replay previously recorded conflict resolutions automatically.
git config rerere.enabled true
git config rerere.autoupdate true

if git merge-base --is-ancestor upstream/main main; then
  echo "main is already up to date with upstream/main"
elif git merge-base --is-ancestor main upstream/main; then
  git merge --ff-only upstream/main
else
  echo "rebasing local main commits onto upstream/main"
  # -X theirs makes conflicting hunks take the local commit being replayed;
  # --empty=drop discards commits upstream has already applied.
  if ! git rebase --empty=drop -X theirs upstream/main; then
    echo "rebase stopped on a conflict that could not be auto-resolved." >&2
    echo "resolve it, then run 'git rebase --continue' and re-run this script." >&2
    exit 1
  fi
fi

# The fork's main history may have been rewritten by the rebase above, so the
# push is a force-with-lease. It fails rather than clobbering commits pushed to
# the fork since this checkout last fetched origin/main. Skip the pre-push hook
# so a sync stays non-interactive; run `just ci` yourself for local commits.
git push --no-verify --force-with-lease origin main
echo "origin/main -> $(git rev-parse --short main)"

if [ -n "$BRANCH" ]; then
  git checkout "$BRANCH" || { echo "no such branch: $BRANCH" >&2; exit 1; }
  git rebase --empty=drop -X theirs main
fi
