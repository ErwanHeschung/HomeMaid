#!/usr/bin/env bash
# Publishes an official plugin: release build, minisign signature with your own key (never in
# CI), a GitHub release on this repo, then prints the version entry to add to index.json in a PR
# on homemaid-plugins.
#
#   tools/publish-plugin.sh <crate> <id> <permission>...    (from core/)
#   tools/publish-plugin.sh tplink-plugin tplink local-network
#
# Needs minisign (key: ~/.minisign/minisign.key, or MINISIGN_KEY=<path>), gh (logged in), python3.
# The version is the crate's; bump it in its Cargo.toml first.
set -euo pipefail
[ $# -ge 2 ] || { sed -n '2,10p' "$0"; exit 1; }
crate=$1 id=$2
shift 2
cd "$(dirname "$0")/.."

version=$(cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name'] == '$crate'))")
# The plugin-api the SDK builds for (`1.2.0` -> `1.2`): the oldest core minor that can run it.
api=$(sed -n 's/^pub const API_VERSION: &str = "\([0-9]*\.[0-9]*\).*/\1/p' plugin-api/src/lib.rs)
cargo build --release -p "$crate" --target wasm32-wasip2
wasm=target/wasm32-wasip2/release/${crate//-/_}.wasm
minisign -S ${MINISIGN_KEY:+-s "$MINISIGN_KEY"} -m "$wasm" -t "$id $version"

tag=$id-v$version
gh release create "$tag" "$wasm" "$wasm.minisig" --title "$id $version" --notes "Plugin \`$id\` $version, plugin-api $api."
url=https://github.com/ErwanHeschung/HomeMaid/releases/download/$tag/$(basename "$wasm")

python3 - "$version" "$api" "$url" "$wasm" "$@" <<'PY'
import hashlib, json, sys
version, api, url, wasm, *permissions = sys.argv[1:]
data = open(wasm, 'rb').read()
print(json.dumps({
    "version": version, "api": api, "permissions": permissions, "url": url,
    "sha256": hashlib.sha256(data).hexdigest(), "signature": open(wasm + '.minisig').read(),
}, indent=2))
PY
