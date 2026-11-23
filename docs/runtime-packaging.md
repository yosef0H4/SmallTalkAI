# Runtime packaging

Parakeet Vulkan is the smallest cataloged GPU runtime for Dictation on Windows and Linux. Windows Read aloud and Image text already share one native ONNX Runtime WebGPU download. Read aloud also uses a Sherpa C API adapter included in the Windows application package. Linux has cataloged Sherpa CPU and CUDA options for both features. [Runtime selection](../src/modules.rs) · [Runtime catalog](../assets/catalog/modules.json) · [Windows adapter packaging](../scripts/build-distribution.ps1)

## Shared CUDA 12.8

The optional NVIDIA download covers Dictation, Read aloud, and Image text. The
base app still bundles Parakeet Vulkan and Sherpa CPU. Both operating systems
use CUDA 12.8, cuBLAS 12.8.3.14, cuDNN 9.10.2, and the same Parakeet and
Sherpa-ONNX versions. Parakeet and Sherpa load the same installed cudart and
cuBLAS files on each system. Sherpa also uses the pinned cuDNN, NVRTC and
cuRAND downloads. OCR and both speech models share Sherpa. The machine supplies
the NVIDIA graphics driver. [Download catalog](../assets/catalog/downloads.json)

OCR uses CPU by default with every runtime. The Device control enables CUDA
or WebGPU only when the user explicitly chooses GPU.

The Linux Parakeet library includes native kernels for the GTX 1070 and
RTX 50-series targets, plus PTX for other supported GPUs. English and Arabic
dictation, PaddleOCR, Kokoro, and Supertonic passed on the GTX 1070 with
driver 550.163.01. Use a CUDA 12.8-era driver for PTX. The Windows upstream
library uses PTX for Pascal. Its imports and GPU targets were inspected and
the application was cross-compiled. Windows GPU execution with these new
pins still needs a native test. [Linux engine release](https://github.com/yosef0H4/simple-stt-runtimes/releases/tag/parakeet-abi6-cuda12.8-linux-x64-v1) · [Windows build workflow](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/.github/workflows/release.yml) · [NVIDIA driver compatibility](https://docs.nvidia.com/deploy/cuda-compatibility/latest/minor-version-compatibility.html)

The Linux build of ONNX Runtime 1.31.1 processed the English and Arabic WebGPU
OCR fixtures on the GTX 1070 test machine, then crashed during worker shutdown. Linux keeps
WebGPU out of the installed runtime catalog while that failure and the native
Sherpa WebGPU adapter remain unresolved.

## Previous runtime packages

The CUDA measurements, dependencies and compatibility notes below describe
the superseded CUDA 13 and Windows Blackwell-only packages.

### Download and installed size

Numbers are bytes for x86-64, before model downloads or system driver installation. Download size includes each catalog dependency once. Extracted size adds the regular file payload the installer retains and excludes links, filesystem overhead and the base installer. WebGPU retains two files from the npm tarball. [Download catalog](../assets/catalog/downloads.json) · [Asset installer](../src/assets.rs)

| Feature and platform | Catalog asset | Download bytes | Extracted bytes |
| --- | --- | ---: | ---: |
| Dictation Vulkan, Windows | `parakeet-vulkan-windows` | 17,945,091 | 59,501,366 |
| Dictation Vulkan, Linux | `parakeet-vulkan` | 18,482,077 | 60,723,648 |
| Dictation CUDA, Windows | `parakeet-cuda-windows` | 599,358,471 | 849,930,182 |
| Dictation CUDA, Linux | `parakeet-cuda` | 563,569,811 | 763,071,368 |
| Read aloud and Image text CPU, Windows | `sherpa-cpu-windows` | 8,032,957 | 23,072,106 |
| Read aloud and Image text CPU, Linux | `sherpa-cpu` | 9,816,899 | 32,416,857 |
| Read aloud and Image text CUDA, Windows | `sherpa-cuda-windows` | 1,387,598,116 | 1,917,197,588 |
| Read aloud and Image text CUDA, Linux | `sherpa-cuda` | 1,284,247,059 | 2,030,272,323 |
| Read aloud and Image text WebGPU, Windows | `paddle-webgpu-windows` | 115,391,311 | 49,723,072 |

Windows Parakeet CUDA consists of `parakeet-cuda-windows` (32,659,792 B), `parakeet-cu12-runtime-windows` (3,037,735 B) and `parakeet-cu12-cublas-windows` (563,660,944 B). Linux Parakeet CUDA ships the engine and NVIDIA libraries together. Sherpa CUDA fetches `sherpa-cuda-windows` (478,110,488 B) and Windows `cuda-cudnn`, `cuda-cublas`, `cuda-nvrtc` and `cuda-runtime` dependencies, or `sherpa-cuda` (254,858,007 B) and the Linux `cuda-cudnn`, `cuda-cublas` and `cuda-nvrtc` dependencies. [Download catalog](../assets/catalog/downloads.json) · [Parakeet v0.5.0 release workflow](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/.github/workflows/release.yml) · [Sherpa v1.13.8 release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8)

## Libraries and sharing

- **Vulkan.** The Parakeet archives carry the engine and documentation. The Windows DLL imports `vulkan-1.dll`, which the PC's Vulkan-capable display driver supplies. The Linux ELF imports `libvulkan.so.1`, which comes from the distribution's loader package. The loader discovers the installed driver. Loading the Vulkan-enabled engine requires the loader even when Dictation uses CPU. Native ONNX Runtime WebGPU can use Dawn's D3D12 or Vulkan backend on Windows and uses Vulkan on Linux, so it can share the system Vulkan stack when it selects that backend. It still needs its own inference library. [Khronos loader guide](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/loader.adoc) · [LunarG Windows prerequisites](https://vulkan.lunarg.com/doc/view/latest/windows/getting_started.html) · [Microsoft WebGPU requirements](https://onnxruntime.ai/docs/execution-providers/WebGPU-ExecutionProvider.html#requirements) · [Fedora loader](https://packages.fedoraproject.org/pkgs/vulkan-loader/vulkan-loader/) · [Arch loader](https://archlinux.org/packages/extra/x86_64/vulkan-icd-loader/files/)
- **Windows CUDA.** Parakeet's custom Blackwell build imports `cudart64_12.dll` and `cublas64_12.dll`. Its cuBLAS dependency also provides `cublasLt64_12.dll`. Sherpa v1.13.8 uses ONNX Runtime 1.28.2, CUDA 13 and cuDNN 9 for both features. Its provider imports `cublas64_13.dll` and `cublasLt64_13.dll`. Sharing app-supplied CUDA libraries would require a tested build of both engines targeting the same CUDA major. The NVIDIA graphics driver can serve both. [Windows build and dependency record](research/windows-runtime-distribution.md) · [ORT CUDA requirements](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html#requirements) · [NVIDIA driver compatibility](https://docs.nvidia.com/deploy/cuda-compatibility/latest/minor-version-compatibility.html)
- **Linux CUDA.** Parakeet needs `libcudart.so.13` and `libcublas.so.13`. Its archive includes cudart 13.0.96 and cuBLAS/cuBLASLt 13.1.1.3 under `$ORIGIN`. The Sherpa CUDA provider needs `libcublas.so.13`, `libcublasLt.so.13`, `libcudart.so.13`, `libcurand.so.10` and the driver-owned `libcuda.so.1`. The catalog installs cuBLAS 13.8.1.7 for Sherpa. The Sherpa archive and its three catalog dependencies supply neither cudart nor cuRAND, so current Linux Sherpa CUDA needs those two libraries on the host search path. The Parakeet worker currently searches its own asset directory, while both reader workers add Sherpa dependencies. [Parakeet release workflow](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/.github/workflows/release.yml) · [STT loader paths](../src/capture/inference_supervisor.rs) · [Reader loader paths](../reader/crates/reader-core/src/native_paths.rs)
- **Windows WebGPU.** The ORT 1.31.1 asset extracts `onnxruntime.dll` (31,736,672 B) and `dxcompiler.dll` (17,986,400 B). The current native package's WebGPU-enabled Sherpa C API DLL adds 4,616,192 B uncompressed and imports the shared ORT DLL. Its build preserves the native worker path. [Windows WebGPU integration and runtime checks](webgpu-runtime-research.md) · [Microsoft npm distribution](https://www.npmjs.com/package/onnxruntime-node/v/1.31.1)

## Packaging opportunities

- Keep the base installer model-free and download acceleration runtimes on demand. Offer Vulkan for Dictation and shared WebGPU for Read aloud and Image text on Windows. Fedora, Arch, and Debian packages declare their distro Vulkan loader. The GPU driver comes from the system.
- The same ORT 1.31.1 tar contains a Linux x64 native WebGPU library (47,552,704 B). This is a candidate for porting the Windows worker integration. The Linux Sherpa build, hardware behavior and minimum driver and distribution requirements still need validation. [Pinned Microsoft Node distribution support table](https://raw.githubusercontent.com/microsoft/onnxruntime/v1.31.1/js/node/README.md)
- A gzip-6 tar of the two existing WebGPU DLLs measured 19,569,452 B before license and notices. A Windows-only package could save about 91 MiB of download without changing the libraries. A release would need their notices, a pinned checksum and the same clean OCR and synthesis worker checks. [ONNX Runtime notices](https://github.com/microsoft/onnxruntime/blob/main/ThirdPartyNotices.txt) · [DirectX Shader Compiler license](https://github.com/microsoft/DirectXShaderCompiler/blob/main/LICENSE.TXT)
- If dual CUDA usage on Linux justifies additional packaging, first validate Parakeet against one pinned shared cuBLAS 13 dependency and account for Sherpa's host cudart and cuRAND requirements. The measured duplicate Parakeet cuBLAS pair occupies 595,785,864 B. A local gzip-6 sample of the upstream Parakeet library, cudart regular file and docs occupied 140,969,323 B before symlinks. A slim release would save download bytes for people who already installed Sherpa's shared CUDA dependencies. Keep using pinned NVIDIA downloads for vendor libraries. The CUDA license lists redistributable components and conditions for distributing them with an application. [NVIDIA CUDA 12.8 distribution terms](https://docs.nvidia.com/cuda/archive/12.8.0/eula/index.html) · [NVIDIA CUDA 13 distribution terms](https://docs.nvidia.com/cuda/archive/13.0.0/eula/index.html)

## CUDA baseline for a shared download

CUDA 12.9 Update 2 is the first candidate to test for unified Windows x64 and
Linux x64 CUDA downloads. It supports targeting Maxwell, Pascal, and Volta
alongside newer cards, including Blackwell, and includes the cuBLAS fixes in
that update. The matching driver baselines are 576.57 on Windows and
575.57.08 on Linux. For a lower driver baseline, CUDA 12.8 already supports
desktop Blackwell and is the version used by the current CUDA 12 ONNX Runtime
builds. CUDA 13 dropped offline compilation and library support for Maxwell,
Pascal, and Volta. [NVIDIA architecture matrix](https://docs.nvidia.com/datacenter/tesla/drivers/latest/cuda-toolkit-driver-and-architecture-matrix.html) · [CUDA 12.9 Update 2 release notes](https://docs.nvidia.com/cuda/archive/12.9.2/cuda-toolkit-release-notes/index.html) · [ONNX Runtime CUDA dependencies](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html)

cuDNN 9.10.2 for CUDA 12.x lists Maxwell through desktop Blackwell. cuDNN 9.11
drops Maxwell, Pascal, and Volta even on CUDA 12. Test the exact cuDNN version
and kernel targets alongside the runtime. Sherpa-ONNX 1.13.8 publishes CUDA 12
and cuDNN 9 builds with ONNX Runtime 1.28.2 for both platforms. Confirm their
required symbols and supported kernel architectures before selecting the
legacy cuDNN build. [cuDNN 9.10.2 matrix](https://docs.nvidia.com/deeplearning/cudnn/backend/v9.10.2/reference/support-matrix.html) · [cuDNN release notes](https://docs.nvidia.com/deeplearning/cudnn/backend/latest/release-notes.html#cudnn-9-11-0) · [Sherpa-ONNX 1.13.8 assets](https://github.com/k2-fsa/sherpa-onnx/releases/expanded_assets/v1.13.8)

Local inspection of Windows `ggml-cuda.dll` with `cuobjdump --list-elf` and
`--list-ptx` found only `sm_120a` kernels. Rebuild Parakeet with the intended
GPU targets on both platforms. Package the CUDA runtime and cuBLAS once per
platform, with cuDNN, cuRAND, and NVRTC as dependencies where the worker
imports them. Test real English and Arabic dictation, Kokoro and Supertonic 3
speech, and OCR on a pre-Turing card and a desktop Blackwell card before
replacing the existing catalog. Shared CUDA remains a candidate. Vulkan and
WebGPU are the cross-vendor runtime choices.

## Verify the measurements

Use the URLs and expected SHA-256 digests in `assets/catalog/downloads.json`. Open tar archives with Python `tarfile` and sum `member.size` for regular files. For ZIPs and wheels, sum `ZipInfo.file_size` for files. Apply `archive_files` when present. The Linux NVIDIA wheel sizes above use the pinned URLs' ZIP central directories. The remaining archives were checked against the catalog SHA-256. Inspect PE imports with `dumpbin /dependents` and ELF imports with `readelf -d`. These checks measure the assets and their link dependencies. Device performance and compatibility require installed-app tests with the intended GPU and driver.

## Bundled defaults and shared NVIDIA package

- First-run setup preselects Parakeet Vulkan for Dictation and Sherpa CPU for Read aloud and Image text. Windows also bundles WebGPU for Read aloud and Image text. Model downloads remain separate.
- One NVIDIA asset job covers the engine-specific CUDA backends and reuses verified components. Removing it restores the bundled runtime for each affected route and preserves the selected models.
- On Windows with an RTX 5080, an isolated package test completed the NVIDIA download from a verified partial cache, ran CUDA speech, and removed NVIDIA from all three runtime lists. Vulkan transcription, CPU and WebGPU speech, and CPU and WebGPU image text also passed with real inputs.
