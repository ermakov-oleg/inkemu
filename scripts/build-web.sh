#!/usr/bin/env bash
# Builds the core as an ES module for the page (and Node) into web/core/: inkcore_wasm.js + inkcore_wasm.wasm.
set -euo pipefail

cd "$(dirname "$0")/.."

scripts/vendor-lua.sh

exports=_main,_malloc,_free,_ik_create,_ik_cmd,_ik_result_len,_ik_frame,_ik_frame_gray,_ik_frame_w,_ik_frame_h,_ik_destroy,_ik_last_error

# link args through `cargo rustc` reach only the final link, so the other crates' builds stay shared with
# the plain wasm32 builds
cargo rustc -p inkcore-wasm --bin inkcore_wasm --target wasm32-unknown-emscripten --release -- \
    -C link-arg=-sSUPPORT_LONGJMP=wasm \
    -C link-arg=-sALLOW_MEMORY_GROWTH=1 \
    -C link-arg=-sMODULARIZE=1 \
    -C link-arg=-sEXPORT_ES6=1 \
    -C link-arg=-sENVIRONMENT=web,worker,node \
    -C link-arg=-sEXPORTED_FUNCTIONS="$exports" \
    -C link-arg=-sEXPORTED_RUNTIME_METHODS=HEAPU8 \
    -C link-arg=-sSTACK_SIZE=8MB

out=target/wasm32-unknown-emscripten/release
mkdir -p web/core
cp "$out/inkcore_wasm.js" "$out/inkcore_wasm.wasm" web/core/
ls -l web/core
