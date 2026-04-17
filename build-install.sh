#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"
git pull --ff-only 2>&1 || echo "[build-install] skip pull (diverged or no upstream)"

# v0.3.2 Cargo.toml uses [patch] sections to redirect several git deps to
# sibling checkouts (../frankensqlite, ../franken_agent_detection,
# ../frankensearch). For a self-contained install, comment those sections
# out so cargo resolves the git URLs pinned in [dependencies] instead.
cp Cargo.toml .Cargo.toml.repomole-bak
cp Cargo.lock .Cargo.lock.repomole-bak

restore_manifests() {
  mv .Cargo.toml.repomole-bak Cargo.toml 2>/dev/null || true
  mv .Cargo.lock.repomole-bak Cargo.lock 2>/dev/null || true
}
trap restore_manifests EXIT

awk '
  BEGIN { in_patch = 0 }
  /^\[patch\./ { in_patch = 1; print "# " $0; next }
  /^\[/ && in_patch { in_patch = 0 }
  in_patch { print "# " $0; next }
  { print }
' .Cargo.toml.repomole-bak > Cargo.toml

# cargo's libgit2 hits "no authentication methods succeeded" fetching
# GitHub git-deps on some macOS setups; route through system git.
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build --release --bin cass

mkdir -p ~/prj/util/bin
rm -f ~/prj/util/bin/cass
cp -p target/release/cass ~/prj/util/bin/
echo "Installed: $(~/prj/util/bin/cass --version)"

cargo clean
echo "Build cache cleaned"
