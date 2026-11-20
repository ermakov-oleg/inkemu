#!/usr/bin/env bash
# Vendors mlua/mlua-sys/lua-src and applies patches/lua32.patch (32-bit Lua numbers). Idempotent.
set -euo pipefail

cd "$(dirname "$0")/.."

MLUA=0.12.2
MLUA_SYS=0.13.0
LUA_SRC=551.0.2

fetch() {
    local name=$1 version=$2
    if [ -d "vendor/$name" ]; then
        return
    fi
    local tmp
    tmp=$(mktemp -d)
    echo "vendor-lua: downloading $name $version"
    curl -fsSL "https://static.crates.io/crates/$name/$name-$version.crate" | tar xz -C "$tmp"
    mkdir -p vendor
    mv "$tmp/$name-$version" "vendor/$name"
    rm -rf "$tmp"
}

patch_hash() {
    if command -v sha256sum >/dev/null; then
        sha256sum patches/lua32.patch | cut -d' ' -f1
    else
        shasum -a 256 patches/lua32.patch | cut -d' ' -f1
    fi
}

hash=$(patch_hash)
if [ -d vendor ] && [ -f vendor/.patch-sha256 ] && [ "$(cat vendor/.patch-sha256)" = "$hash" ]; then
    echo "vendor-lua: up to date"
    exit 0
fi

# a changed patch (or a half-done earlier run) is applied to fresh sources
rm -rf vendor
fetch mlua "$MLUA"
fetch mlua-sys "$MLUA_SYS"
fetch lua-src "$LUA_SRC"

if ! git apply patches/lua32.patch; then
    rm -rf vendor
    echo "vendor-lua: patches/lua32.patch does not apply to mlua $MLUA / mlua-sys $MLUA_SYS / lua-src $LUA_SRC" >&2
    exit 1
fi

echo "$hash" > vendor/.patch-sha256
# mlua-sys keeps the compiled C Lua in target/: rebuild it so it matches the new patch
cargo clean -p mlua-sys 2>/dev/null || true
echo "vendor-lua: patched"
