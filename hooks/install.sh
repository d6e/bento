#!/usr/bin/env bash
set -e

REPO_ROOT="$(git rev-parse --show-toplevel)"
git config core.hooksPath "$REPO_ROOT/hooks"
echo "Git hooks installed (core.hooksPath set to hooks/)"
