#!/usr/bin/env bash
set -euo pipefail

ctl=${1:?expected path to smalltalkai-ctl}
destination=${2:?expected adapter output directory}
scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/smalltalkai-sherpa-webgpu.XXXXXXXX")"
trap 'rm -rf -- "$work"' EXIT

revision=11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf
git clone --quiet --depth 1 --branch v1.13.8 https://github.com/k2-fsa/sherpa-onnx.git "$work/source"
test "$(git -C "$work/source" rev-parse HEAD)" = "$revision"
git -C "$work/source" apply "$scripts/sherpa-webgpu.patch"

# Link to the same verified ONNX Runtime that the app downloads for OCR and speech.
XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache" \
    "$ctl" assets install paddle-webgpu-linux > "$work/runtime.json"
runtime="$(python3 -c 'import json, sys; print(json.load(sys.stdin)["directory"])' < "$work/runtime.json")"

# Sherpa v1.13.8 uses the ORT 1.28 C ABI. Only the headers from its pinned
# dependency are needed since the WebGPU library implements that ABI.
curl --fail --silent --show-error --location --retry 2 \
    https://github.com/csukuangfj/onnxruntime-libs/releases/download/v1.28.2/onnxruntime-linux-x64-glibc2_17-Release-1.28.2.zip \
    -o "$work/headers.zip"
printf '%s  %s\n' c4f8994d56191d9d2c92a961b39fe790459f2c5d155f912b239506ea31359534 "$work/headers.zip" | sha256sum --check
mkdir "$work/include"
python3 - "$work" <<'PY'
import sys
from pathlib import Path
from zipfile import ZipFile

work = Path(sys.argv[1])
with ZipFile(work / "headers.zip") as archive:
    for name in archive.namelist():
        if "/include/" in name and name.endswith(".h"):
            (work / "include" / Path(name).name).write_bytes(archive.read(name))
PY

SHERPA_ONNXRUNTIME_INCLUDE_DIR="$work/include" SHERPA_ONNXRUNTIME_LIB_DIR="$runtime/lib" \
    cmake -S "$work/source" -B "$work/build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_SKIP_RPATH=ON -DBUILD_SHARED_LIBS=ON -DSHERPA_ONNX_ENABLE_BINARY=OFF \
    -DSHERPA_ONNX_ENABLE_PORTAUDIO=OFF -DSHERPA_ONNX_ENABLE_WEBSOCKET=OFF \
    -DSHERPA_ONNX_ENABLE_SPEAKER_DIARIZATION=OFF
cmake --build "$work/build" --parallel "${CARGO_BUILD_JOBS:-2}" --target sherpa-onnx-c-api
install -Dm755 "$work/build/lib/libsherpa-onnx-c-api.so" "$destination/libsherpa-onnx-c-api.so"
install -m644 "$work/source/LICENSE" "$destination/Sherpa-ONNX-LICENSE.txt"
install -m644 "$work/build/_deps/espeak_ng-src/COPYING" "$destination/eSpeak-NG-COPYING.txt"
