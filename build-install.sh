#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"
git pull
cargo build --release
mkdir -p ~/prj/util/bin
cp target/release/cass ~/prj/util/bin/
echo "Installed: $(~/prj/util/bin/cass --version)"
