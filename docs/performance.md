# SmallTalkAI on one Windows PC

These are ballpark figures from a Core i5-12600KF with 32 GiB RAM and an NVIDIA RTX 5080 with 16 GiB VRAM. The requests below give an idea of what using the app feels like on this machine.

## The whole app

| App state | RAM | Dedicated VRAM |
| --- | ---: | ---: |
| Idle, Settings closed | 119 MiB | 0 MiB |
| After a dictation test, Parakeet 110M Q8 loaded on Vulkan | 200 MiB | 226 MiB |

The table shows the median RAM total across seven snapshots per state, with VRAM measured for the same set of processes. RAM is the sum of the running app processes’ working sets, including AutoHotkey (29 MiB at idle), capture, reader, both control processes, and the inference worker when loaded. Shared pages can be counted by more than one process. In three separate first-request runs, the median of the sampled whole-app RAM peaks was 481 MiB, with peaks ranging from 344 to 496 MiB.

In a fresh, isolated launch of the same Windows app, the CPU dictation run went from **85 MiB idle to 283 MiB loaded**, with no dedicated VRAM. The matched Vulkan run went from **85 MiB idle to 151 MiB loaded**, with **226 MiB dedicated VRAM** once the model loaded. Both runs include AutoHotkey and every other app process. The profile had its shortcuts and startup registration disabled for the measurement.

## Model and runtime samples

Each timing is a median of three first requests and five requests with the model already loaded. A first request includes starting the worker and loading the model. Timings cover producing the result and exclude recording and audio playback. RAM is the highest peak working set reported by Windows for the worker across the three launches. VRAM is the worker’s dedicated GPU allocation after the warm requests, measured with the Windows GPU process counter. Both are in MiB.

Dictation starts loading the model as soon as you begin recording. If it finishes loading while you’re talking, the warm timing is closer to what you feel after releasing the shortcut. Short recordings can still leave some of that loading time after release. With read-aloud, an unloaded voice model starts loading when you trigger the shortcut, so startup is part of the wait. The speech rows time generation of the complete sample sentence. Measuring when you first hear audio would require a playback measurement.

| Workload | Runtime | First request (ms) | Warm (ms) | Peak worker RAM | Loaded worker VRAM |
| --- | --- | ---: | ---: | ---: | ---: |
| Parakeet 110M Q8, 7.4 s recording | CPU | 221 | 146 | 218 | 0 |
| | Vulkan | 2,377 | 64 | 395 | 227 |
| | CUDA | 1,166 | 39 | 528 | 486 |
| PaddleOCR Arabic + English, 640 × 360 | CPU | 348 | 126 | 93 | 0 |
| | CUDA | 1,954 | 249 | 927 | 304 |
| | WebGPU | 1,129 | 19 | 283 | 174 |
| PaddleOCR Arabic + English, 1920 × 1080 | CPU | 415 | 203 | 153 | 0 |
| | CUDA | 1,747 | 253 | 944 | 304 |
| | WebGPU | 1,167 | 29 | 227 | 282 |
| Kokoro, 4.2 s of generated speech | CPU | 6,211 | 1,995 | 613 | 0 |
| | CUDA | 4,977 | 190 | 1,108 | 1,304 |
| | WebGPU | 4,089 | 605 | 469 | 822 |
| Supertonic 3, 5.5 s of generated speech | CPU | 1,289 | 728 | 295 | 0 |
| | CUDA | 2,173 | 87 | 946 | 644 |
| | WebGPU | 2,370 | 225 | 308 | 318 |

## CPU and GPU activity

CPU time adds up the worker's time across its threads. CPU percentage is its average share of this PC's 16 logical processors during back-to-back warm requests. GPU percentage is the average activity of the worker's busiest GPU engine over two one-second samples in that same run. These percentages show sustained use for the fixture, so a single request is usually just a brief burst. [Microsoft's GPU counters](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/)

| Workload | Runtime | First-request CPU time (ms) | Warm CPU time per request (ms) | Warm CPU | Warm GPU engine |
| --- | --- | ---: | ---: | ---: | ---: |
| Parakeet 110M Q8 | CPU | 1,109 | 1,204 | 48.7% | 0.0% |
| Parakeet 110M Q8 | Vulkan | 359 | 55 | 4.7% | 17.5% |
| Parakeet 110M Q8 | CUDA | 703 | 39 | 5.3% | 18.6% |
| PaddleOCR 640x360 | CPU | 625 | 463 | 21.0% | 0.0% |
| PaddleOCR 640x360 | CUDA | 2,000 | 253 | 6.0% | 59.7% |
| PaddleOCR 640x360 | WebGPU | 1,516 | 28 | 9.7% | 31.4% |
| PaddleOCR 1920x1080 | CPU | 953 | 727 | 22.2% | 0.0% |
| PaddleOCR 1920x1080 | CUDA | 1,875 | 283 | 6.5% | 59.7% |
| PaddleOCR 1920x1080 | WebGPU | 1,484 | 70 | 15.9% | 25.4% |
| Kokoro | CPU | 4,797 | 3,380 | 12.5% | 0.0% |
| Kokoro | CUDA | 3,141 | 189 | 6.1% | 38.6% |
| Kokoro | WebGPU | 3,344 | 88 | 0.9% | 93.1% |
| Supertonic 3 | CPU | 3,859 | 2,438 | 21.5% | 0.0% |
| Supertonic 3 | CUDA | 3,719 | 180 | 12.3% | 35.4% |
| Supertonic 3 | WebGPU | 4,172 | 588 | 18.1% | 32.1% |

The runs used the same fixtures as the latency tests. CPU time for the first request comes from one fresh worker launch per case. The tests ran without a game or another competing workload.

## Which runtime would I start with?

Start with CPU for occasional dictation or a single screenshot when keeping GPU memory free matters. If your goal is to leave the CPU free for other work, use Vulkan for dictation and WebGPU for OCR or Kokoro. CUDA is an option for faster warm speech generation, especially Kokoro, when you can spare the extra memory.

- **Dictation.** CPU had the quickest first request (221 ms), but repeat dictation averaged 49% CPU on the CPU runtime and about 5% on both GPU runtimes. CUDA saved 25 ms over Vulkan per warm request and used about 260 MiB more VRAM. Its first request was about 1.2 seconds faster than Vulkan’s.
- **OCR.** CPU was quickest for one scan from a cold start. WebGPU was quickest warm and did the least CPU work per scan at both image sizes. CUDA spent longer on each scan and used more memory. It showed lower average CPU than WebGPU under continuous requests because it finished fewer scans in the same period.
- **Read aloud.** If you mostly select a passage every so often, pay attention to the first request. Kokoro generated the first sample fastest on WebGPU (4.1 s) and Supertonic 3 did so on CPU (1.3 s). CUDA generated warm samples fastest for both voices. For Kokoro, it saved about 415 ms over WebGPU while using twice as much CPU time and 482 MiB more VRAM. For Supertonic 3, it saved about 138 ms and cut CPU time per request from 588 ms to 180 ms, with roughly twice the VRAM.

The OCR images had the same short English text at both sizes. Different text, image content, hardware, and worker reuse can change these results.

The sample covers the Windows runtimes available for these models. The [measurement script](../scripts/bench-windows.py) runs against installed workers. For the Parakeet CUDA sample, the matching CUDA 12 and cuBLAS DLLs were staged beside Parakeet in a temporary folder.

### Parakeet on CPU across packages

I forced CPU inference for each package and alternated their run order. In the direct comparison, the dedicated CPU build took 231 ms on the first request and 150 ms warm. The Vulkan package in CPU mode took 224 ms and 144 ms, with about the same peak RAM (215 and 216 MiB). A separate pairing measured 616 ms warm for the CUDA package in CPU mode and 139 ms for the Vulkan package. All three used zero dedicated VRAM in these tests. The single **CPU** choice for dictation uses the Vulkan package when it’s installed. [Parakeet v0.5.0 runtime builds](https://github.com/mudler/parakeet.cpp/releases/tag/v0.5.0)

In five more alternating trials of six warm requests each, the dedicated build and the Vulkan build both took 142 ms per warm request and used 50% of the machine’s CPU capacity during back-to-back requests. The dedicated build used 1,146 ms of CPU time across its threads per request, compared with 1,188 ms for Vulkan in CPU mode.

### OCR and read-aloud on CPU across packages

These paired runs forced CPU inference in both packages. Each run measured the first request and three warm requests, with package order reversed for the second run. Each cell lists the dedicated CPU package first and the WebGPU package in CPU mode second. Both used zero dedicated VRAM.

| Workload | First request (ms) | Warm request (ms) | Warm CPU usage | Peak worker RAM (MiB) |
| --- | ---: | ---: | ---: | ---: |
| PaddleOCR, 1920 × 1080 | 396 / 432 | 188 / 186 | 22.1% / 22.4% | 153 / 159 |
| Kokoro | 2,818 / 3,044 | 1,620 / 1,615 | 12.48% / 12.47% | 613 / 618 |
| Supertonic 3 | 1,282 / 1,601 | 674 / 645 | 22.1% / 22.2% | 294 / 299 |

The dedicated CPU package had faster first requests and used about 5 MiB less RAM in each workload. Warm CPU usage was close in all three. Settings uses the dedicated package for **CPU** and requests GPU acceleration when you choose **WebGPU**.
