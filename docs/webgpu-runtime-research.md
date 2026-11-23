# WebGPU for Image text and Read aloud

## Windows OCR prototype

The `smalltalkai-ocr` worker can opt into WebGPU with `--webgpu`.
It uses the existing PaddleOCR detector, recognizer and
postprocessing. The prototype requires WebGPU compiled into the native ONNX
Runtime library because the pinned Rust binding selects the provider through
the legacy session API. The separately registered plugin needs the device-based
session API.

Run the worker check headlessly with a PaddleOCR Arabic + English model
directory and the native ONNX Runtime libraries on disk.

```sh
cargo build --release -p smalltalkai-ocr
python scripts/test-reader-workers.py --bin-dir target/release --ocr-model <model-dir> --ocr-runtime <runtime-dir> --device gpu --webgpu
```

On Windows with an RTX 5080, the native libraries from Microsoft's ONNX Runtime
1.31.1 package passed the light, dark and Arabic fixtures and exited cleanly.
The trace showed 272 detector nodes and 339 recognizer nodes assigned to WebGPU
and recorded GPU dispatches. The English light fixture took 3.86 s on the first
WebGPU request and 1.32 s on CPU. The warm English dark fixture took 0.16 s on
WebGPU and 0.58 s on CPU. These single-run observations include trace overhead.
An earlier 1.26.0 runtime recognized the fixtures and returned a nonzero exit
at teardown. [Microsoft ONNX Runtime 1.31.1 distribution](https://www.npmjs.com/package/onnxruntime-node/v/1.31.1)

Windows x64 Settings recommends WebGPU for **Image text** and **Read aloud**
on compatible NVIDIA, AMD, and Intel GPUs. Both features share one verified
download of ONNX Runtime and DXCompiler. The installer provides the
WebGPU-enabled Sherpa C API separately. Linux, AMD, and Intel hardware remain
untested.

### Native WebGPU adapter selection on Windows

- The pinned npm runtime exposes the built-in provider as `WebGPU` through
  `SessionOptionsAppendExecutionProvider`. This API takes short option keys and
  adds the `ep.webgpuexecutionprovider.` prefix itself. OCR can pass short keys
  through `ort::ep::ArbitrarilyConfigurableExecutionProvider` and Sherpa can pass
  them in its existing provider-option arrays before creating model sessions.
  [Upstream native perftest](https://github.com/microsoft/onnxruntime/blob/main/onnxruntime/test/perftest/ort_test_session.cc)
  · [OCR registration](../reader/crates/smalltalkai-ocr/src/webgpu.rs)
  · [Sherpa registration](../scripts/build-sherpa-webgpu.ps1)
- `powerPreference` accepts `high-performance` or `low-power`. Dawn uses it
  as a hint, and ONNX Runtime 1.31.1 already defaults to high performance.
  `deviceId` identifies the shared WebGPU context. For physical selection,
  upstream documents `adapterIndex` as a zero-based index into Dawn's adapter
  list for the requested backend and power preference. The bundled DLL ran
  Kokoro with index 0 and rejected index 999 while reporting three adapters.
  **Device → Choose GPU adapter** passes the same index to both workers.
  An index can change with backend, drivers, or hardware.
  [ORT 1.31.1 context defaults](https://github.com/microsoft/onnxruntime/blob/v1.31.1/onnxruntime/core/providers/webgpu/webgpu_context.h#L135-L180)
  · [ORT WebGPU adapter options](https://github.com/microsoft/onnxruntime/blob/main/plugin-ep-webgpu/python/onnxruntime_ep_webgpu/README.md)
  · [Dawn adapter enumeration](https://dawn.googlesource.com/dawn/+/refs/heads/main/docs/dawn/features/adapter_options.md)
- For the separate WebGPU plugin, `GetEpDevices` discovers entries used to
  attach the provider. The plugin accepts one EP device per append and
  selects the physical Dawn adapter independently. Keep the built-in
  registration path for the pinned npm runtime. On a hybrid Windows host,
  exercise OCR and speech in fresh worker processes and correlate actual
  WebGPU execution with the physical adapter handling each worker's GPU
  commands. An ORT operator-placement trace identifies the provider only.
  [ORT plugin API](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html#usage)
  · [ORT WebGPU adapter options](https://github.com/microsoft/onnxruntime/blob/main/plugin-ep-webgpu/python/onnxruntime_ep_webgpu/README.md)
  · [Windows GPU process counters](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/)

## WebGPU for speech synthesis

- **Kokoro v1.0** runs in a WebGPU-enabled browser through Kokoro.js and Transformers.js. Hexgrad documents the `Kokoro-82M-v1.0-ONNX` export, the `webgpu` device, and `fp32` as its recommended WebGPU precision. [Kokoro.js usage](https://github.com/hexgrad/kokoro/blob/main/kokoro.js/README.md#usage)
- **Supertonic 3** has an official browser demo that loads the WebGPU build of ONNX Runtime Web 1.23.0, selects WebGPU in a secure context when the browser exposes it, and retries model loading with WASM if WebGPU initialization fails. Supertone preserves a local example and the v3 weights in its archive. [Demo runtime import](https://huggingface.co/spaces/Supertone/supertonic-3/blob/main/index.html#L28) · [Demo provider selection](https://huggingface.co/spaces/Supertone/supertonic-3/blob/main/script.js#L1319) · [Archived web example](https://github.com/supertone-oss-archive/supertonic/blob/main/web/README.md)
- **SmallTalkAI Read aloud** uses native Sherpa-ONNX v1.13.8 for both models. The Windows installer builds one Sherpa C API adapter that selects WebGPU before creating inference sessions. The adapter uses the same ONNX Runtime libraries as Image text. [Runtime catalog](../assets/catalog/modules.json) · [Build script](../scripts/build-sherpa-webgpu.ps1) · [TTS worker](../reader/crates/smalltalkai-tts/src/main.rs)
- **Native Sherpa integration** can register a compatible ONNX Runtime WebGPU plugin and attach the selected GPU device before creating model sessions. Kokoro creates one session and Supertonic creates four with shared session options. Sherpa issue #3665 is open and links a prototype branch whose author reported GPU speaker diarization on an M2 Mac. [Microsoft native WebGPU API](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html#usage) · [Kokoro session](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/offline-tts-kokoro-model.cc#L27-L37) · [Supertonic sessions](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/offline-tts-supertonic-model.cc#L210-L227) · [Sherpa issue #3665](https://github.com/k2-fsa/sherpa-onnx/issues/3665) · [Prototype provider code](https://github.com/benniekiss/sherpa-onnx/blob/c8f2f33cd0c82bcded0819d34e0475c74b710866/sherpa-onnx/csrc/webgpu-provider.cc)

On Windows with an RTX 5080, a headless ONNX Runtime C API probe ran the
installed Kokoro `model.onnx` with the native ONNX Runtime 1.31.1 DLL from
the Image text WebGPU download. The probe used the phonemes for “hello,”
the `af_heart` style from the installed `voices.bin`, and speed 1.0. CPU and
WebGPU each produced 31,800 finite, non-silent samples in both runs. The
waveform correlation was 0.99984 on the first run and 0.99962 on the repeat.
Both WebGPU traces recorded 1,866 GPU node executions and 508 CPU node
executions.

The packaged CLI rendered Kokoro and Supertonic 3 INT8 to WAV on the same
Windows machine using `assets select tts` and `media speak --output`. Native
Sherpa traces confirmed GPU dispatch for Kokoro's session and all four
Supertonic sessions. Both workers shut down cleanly. A disposable desktop
Settings profile verified System → Factory reset, cancellation of the
confirmation, deletion of the copied models and configs, and return to
Appearance in first-time setup.

## Runtime approach

Prototype the native ONNX Runtime WebGPU execution provider inside the existing disposable workers. It uses Dawn directly, with Vulkan on Linux and D3D12 or Vulkan on Windows. Settings can remain an installer and selector for the optional runtime. The worker can load the runtime on demand and release its GPU allocations when the existing idle policy exits the worker. [ONNX Runtime WebGPU provider](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html) · [SmallTalkAI worker architecture](architecture.md)

| Approach | Fit for SmallTalkAI |
| --- | --- |
| Native ONNX Runtime and Dawn | Reuses native workers, model files and framed IPC. Microsoft documents library registration and GPU device selection from native code. [Native WebGPU API](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html#usage) |
| Hidden Tauri WebView with ONNX Runtime Web | A viable browser experiment where the WebView exposes WebGPU. Tauri uses WebView2 on Windows and WebKitGTK on Linux, so the worker would inherit system-specific WebGPU support and start a WebView process to run inference. [Tauri architecture](https://v2.tauri.app/concept/architecture/) · [ONNX Runtime Web GPU guide](https://onnxruntime.ai/docs/tutorials/web/ep-webgpu.html) |
| Node or Bun worker | Node can use ONNX Runtime's native addon. Bun exposes Node-API for native addons, but the exact addon and provider would need testing with Bun. Both add a JavaScript host around a native inference provider. [ONNX Runtime Node binding](https://onnxruntime.ai/docs/get-started/with-javascript/node.html) · [Bun Node-API](https://bun.com/docs/runtime/node-api) |

## Packaging and integration

- Microsoft publishes native plugin builds for Windows and Linux x64 and ARM64. Pin the core and plugin together and verify registration against that pair. Linux needs the Vulkan loader and a compatible graphics driver. Upstream 0.6.0 wheels are approximately 7.2 MB for Linux x64 and 18.1 MB for Windows x64, which gives an estimate for the plugin before adding the ONNX Runtime core. On Windows, use a standard D3D12 or Vulkan build and verify it on a normal installation. Agility-enabled upstream builds use a preview D3D12 SDK that requires Developer Mode. [Microsoft package prerequisites](https://www-0.nuget.org/packages/Microsoft.ML.OnnxRuntime.EP.WebGpu/) · [Microsoft 0.6.0 release files](https://pypi.org/project/onnxruntime-ep-webgpu/0.6.0/)
- **PaddleOCR first.** The OCR worker already loads ONNX Runtime dynamically and creates detector and recognizer sessions in Rust. Its pinned `oar-ocr` has a WebGPU provider variant. Verify whether the existing provider wrapper works with the selected native build. If using the separately distributed plugin, register the library, enumerate its devices and attach the chosen device to both sessions through the current ORT device API. [OCR worker](../reader/crates/smalltalkai-ocr/src/main.rs) · [OAR OCR provider flags](https://github.com/GreatV/oar-ocr/blob/main/docs/features.md) · [ORT plugin API](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html#usage)
- **Shared speech implementation.** The Windows build adds a WebGPU provider branch to Sherpa session setup. Kokoro and Supertonic use the same branch and retain Sherpa's model loading, text preparation, voice selection and audio output. [TTS worker](../reader/crates/smalltalkai-tts/src/main.rs) · [Sherpa session setup](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/session.cc)

## Acceptance checks

1. Profile one representative screenshot through the existing PaddleOCR detector and recognizer. Compare the recognized text with the CPU output and check that ORT assigns meaningful graph work to the selected hardware adapter. Record first-use and warm latency plus RAM and VRAM. Then repeat on matched fixtures across AMD and Intel hardware on Windows and Linux. Use NVIDIA as an additional comparison.
2. For Kokoro, use the same model, voice and short and long passages for CPU and WebGPU. Compare time to first audio, sustained synthesis speed, audio and pronunciation quality, peak memory and warm-worker reuse. Performance depends on the workload, adapter, driver and operator placement. [ORT profiling](https://onnxruntime.ai/docs/performance/tune-performance/profiling-tools.html)
3. Exercise unavailable hardware, CPU selection, cancellation, worker crashes and idle cleanup with the normal app paths. The Vulkan option for Dictation is a separate Parakeet runtime. [Runtime catalog](../assets/catalog/modules.json) · [Existing validation suite](testing.md)
