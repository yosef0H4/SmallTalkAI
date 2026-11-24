# Long dictation

## What failed

On Windows with Parakeet TDT-CTC 110M on Vulkan, the installed app recorded 605.83 seconds through a virtual audio cable and failed during recognition with `ErrorOutOfDeviceMemory`. The shell received no transcript. A 5.5-second recording in the same harness reached the Windows edit control and matched the captured transcript exactly. The installed file CLI processed the same 10-minute WAV in 8.09 seconds. It targets 25-second chunks, searches for a quiet cut near that target, and caps individual chunks at 35 seconds. The tested live path submits the entire PCM buffer. [File chunking](../../src/cli.rs#L218) · [Live inference](../../src/bin/smalltalkai_capture.rs#L1465)

This establishes an out-of-memory failure for the full-recording call on the tested hardware and runtime. Shortened transcripts reported from other recordings need separate reproduction.

## Bounded recognition with the current models

Share the file CLI's segmentation policy with live dictation. Send one bounded PCM chunk at a time through the existing inference worker. Keep the selected model in that worker and join the text in order. Short recordings still fit in one inference call. Start with the CLI's tested per-chunk durations and validate peak VRAM on lower-memory devices. This bounds the per-call recognition workload. If capture retains the original PCM until release, its audio buffer still grows with recording length.

Prefer pauses near the target length and enforce a hard maximum for uninterrupted speech. Preserve every sample and always submit the final short segment. If tests reveal errors at forced cuts, give adjacent chunks a little shared context and merge only the words attributable to the overlap. Preserve intentional repeated words. NVIDIA documents overlapping buffered inference for offline CTC, RNN-T, and TDT models. Hugging Face's CTC example discards overlap at the logits level, which requires access to those logits. [NeMo long audio inference](https://docs.nvidia.com/nemo/speech/nightly/asr/inference.html#long-audio-inference) · [CTC chunking with stride](https://huggingface.co/blog/asr-chunking#chunking-with-stride)

Next, decode completed chunks while recording so the stop action can finish the tail and any backlog. Keep capture responsive, use one worker with a bounded pending queue, and retain unread audio if inference falls behind. Dispatch completed chunks sequentially and join by recording order. If a chunk fails, report an incomplete result and keep it available for recovery. Deliver one complete transcript when the user stops. The existing waveform tooltip can show one or two lines of recently finalized text, then show progress for outstanding work at stop.

## Native streaming

SmallTalkAI targets Parakeet.cpp v0.5.0 C ABI 6. On the installed Windows Vulkan DLL, inspection verified `parakeet_capi_stream_begin`, `feed`, `feed_json`, `finalize`, and `free` exports. The app's Rust wrapper currently uses the batch entry points. The streaming API requires a compatible cache-aware model on this release. The tested TDT-CTC 110M model uses full attention and produces punctuation and capitalization. [Pinned streaming API](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/include/parakeet_capi.h) · [Compatibility check](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/src/parakeet_capi.cpp#L515-L544) · [TDT-CTC model card](https://huggingface.co/nvidia/parakeet-tdt_ctc-110m) · [Rust wrapper](../../src/infer/parakeet_native.rs#L35)

The 120M EOU model is a candidate for an optional fast English preview in the same worker. It emits newly finalized text and end-of-utterance signals, with no punctuation or capitalization. Use those signals for display grouping and the user's stop command for dictation completion. Call `finalize` to flush the tail. The pinned runtime grows and recopies the session's mel feature buffer as new frames arrive. Measure long-run RAM and processing latency before adopting this path. [NVIDIA 120M EOU model card](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1) · [Streaming session storage and copying](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/src/parakeet_capi.cpp#L41-L93)

## Acceptance checks

Replay the existing 10-minute fixture through the real app and compare the entire inserted text with the result from the same recording. Check that the opening, middle sections, and final line survive. Additional coverage for CUDA, longer sessions, and accented speech would help establish where the results generalize.

## Windows implementation and comparison

The worker now partitions long PCM requests with the same quiet-cut policy as the media CLI. Each inference call is bounded at 35 seconds before end padding. The worker reuses one loaded model and returns a single transcript after every chunk succeeds. Short recordings use the existing single inference call. Capture still buffers PCM through stop. [Shared segmentation and inference](../../src/infer/chunks.rs) · [Worker entry point](../../src/bin/smalltalkai_infer.rs)

The full app replayed the 605.83-second WAV through the VB-Audio virtual cable with the patched inference worker. Smart Paste inserted 11,922 characters identical to the capture transcript about six seconds after stop. End-to-end word error rate was 4.12% on the 2,159-word reference. The final sentence was present with “marks” recognized as “mocks.” The initial literal sentinel check flagged that word substitution. An independent comparison verified the complete delivery and examined the text of all 36 sections.

The matched Vulkan runs used the same 16 kHz mono PCM on Windows with RTX 5080. RAM and VRAM are sampled peaks for the inference process using private resident memory and WDDM dedicated usage. Times are three-run medians, with both EOU runs shown as a range.

| Method | WER | Work after input | Total recognition | Private RAM | Dedicated VRAM |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parakeet 110M Q8, quiet cuts in worker | 3.61% | 5.39 s | 5.39 s | 214–277 MiB | 335–978 MiB |
| Same Parakeet, WebRTC VAD cuts | 3.61% | 5.41 s | 5.41 s | 203 MiB | 335–337 MiB |
| Native Nemotron English 600M Q8 | 3.84% | 0.04 s final feed and flush | 46.33 s | 283–376 MiB | 1,274–1,906 MiB |
| Native Parakeet Realtime EOU English 120M Q8 | 8.85% | 0.034–0.043 s final feed and flush | 59.78–65.25 s | 192–235 MiB | 528 MiB |

For Nemotron, the native C SDK streamed the audio unpaced in 200 ms input blocks. The final feed and flush indicate tail work when audio has already been processed. They exclude desktop paste and do not measure real-time 10-minute release latency. The prebuilt Windows Vulkan SDK selected 160 ms internal steps. Test setup used NVIDIA’s official English-only 700 MB GGUF. [NeMo-Speech.cpp v0.3.0](https://github.com/NVIDIA/NeMo-Speech.cpp/releases/tag/v0.3.0) · [Model file](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b/tree/main)

WebRTC VAD used mode 2 with 20 ms frames and preferred gaps of at least 100 ms in the existing 20–30 second cut search. It used the existing quiet-window cut where no qualifying gap was present. The synthetic fixture used that fallback at every cut. A 212-second human speech fixture assembled from 24 LibriSpeech validation excerpts changed eight cuts. WER was 4.23% with quiet cuts, 4.64% with WebRTC, 4.23% with Nemotron, and 7.46% with EOU. VAD runs split PCM outside the worker, so the worker's RAM figures exclude that host-side buffer. [WebRTC VAD](https://github.com/wiseman/py-webrtcvad#how-to-use-it) · [LibriSpeech sample](https://huggingface.co/datasets/hf-internal-testing/librispeech_asr_demo)

EOU streamed through the installed Parakeet Vulkan DLL's native C API. The first minute took 4.32–4.45 seconds to decode and the last minute took 7.28–8.42 seconds. The pinned implementation recopies accumulated mel features as the session progresses. The benchmark measures the aggregate cost of each minute. [Streaming session storage and copying](https://github.com/mudler/parakeet.cpp/blob/v0.5.0/src/parakeet_capi.cpp#L41-L93)

On a 4.61-second synthetic phrase, the repeated Vulkan tests after the initial screening gave roughly 49 ms after input for Parakeet and 57–66 ms on the paced native Nemotron stream. Each model made one word error in the 16-word phrase. The second EOU run finished 42 ms after input and made two word errors. Its cold first trial took 1.40 seconds after input, including model startup delay. CPU spot checks on this clip used 195 MiB peak private RAM with Parakeet, 344 MiB with Nemotron, and 184 MiB with EOU.

The official Nemotron ASR checkpoints reviewed start at 600M parameters. The newer Labs Unified model has better published scores on some English evaluations and uses a 600M multilingual checkpoint. The 120M Parakeet Realtime EOU is English-only and emits text without punctuation and capitalization. A 129 MB EOU Q4_K variant is available. These results use the 176 MB Q8 variant. WebRTC VAD and both streaming paths were confined to the benchmark. [NVIDIA Labs Unified evaluation](https://huggingface.co/nvidia/nemotron-labs-unified-asr-0.6b#comparison-with-already-released-models) · [Parakeet EOU](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1) · [EOU quantizations](https://huggingface.co/mudler/parakeet-cpp-gguf/tree/main)

The follow-up PowerShell test inserted all 96 recognized characters from a 5.5-second recording into a scratch prompt in 0.7 seconds. VS Code saved no pasted text in the isolated desktop for either the 70-second recording or a 5.5-second retry. Delivery to VS Code remains unverified.

## Matched WebRTC comparison

The earlier WebRTC result sampled the inference worker and used a different PCM delivery path for quiet cuts. The follow-up split PCM in the same host for both strategies, then sent each piece to the same Parakeet 110M Vulkan worker. Each row uses three runs with alternating order. RAM is the median of the peak combined private resident RAM of the benchmark host and worker, in MiB.

| Audio | Quiet cuts WER | WebRTC WER | Quiet cuts RAM | WebRTC RAM |
| --- | ---: | ---: | ---: | ---: |
| 605.8 s synthetic | 3.61% | 3.61% | 241 | 247 |
| 212 s LibriSpeech excerpts | 4.23% | 4.64% | 216 | 235 |
| Same human excerpts with +12 dB SNR | 5.65% | 5.04% | 230 | 236 |
| Same human excerpts with +5 dB SNR | 9.68% | 11.09% | 236 | 228 |

The noise mix used reproducible simulated broadband noise, hum and brief bursts. Both strategies produced identical cuts for the synthetic clip, where the WebRTC run found no suitable pause. Dedicated VRAM varied by hundreds of MiB across those otherwise equivalent runs, so the difference cannot establish a VAD memory saving. The human and noise runs stayed near 324–334 MiB and WebRTC changed eight cuts. Results support keeping the quiet-window policy as the default for now.

The standalone Windows Rust probe statically linked the WebRTC C detector through `webrtc-vad` 0.4.0 and matched its Python binding on all 10,607 frames. The detection pass took 14 ms for 212 seconds of mono 16 kHz audio. Integration would add a detector state per long recording and use speech-free 20 ms frames to rank candidate cuts. The existing quiet cut still supplies a fallback and every audio sample goes to the worker. [Native WebRTC implementation](https://webrtc.googlesource.com/src/%2B/f8e91dfdf4ddb91fe8308e33e4d960994612217a/common_audio/vad/webrtc_vad.c) · [Rust API](https://docs.rs/webrtc-vad/0.4.0/webrtc_vad/)

## Processing chunks while recording

Windows and Linux use the same capture service. In Chunked mode, local shortcut dictation submits the first chunk once buffered audio exceeds 35 seconds, then processes one chunk at a time. Cuts match the release-only policy for the same PCM. The recording buffer releases a chunk's audio only after recognition succeeds. A background failure leaves that audio available to retry after stop. Under the waveform, the overlay shows the latest completed chunk number and the end of its transcript. An ellipsis marks any trimmed beginning. Releasing the key clears the preview and produces one combined transcript. Short recordings, remote dictation and Settings previews submit audio on stop. [Recording buffer](../../src/capture/recording_chunks.rs) · [Shared overlay](../../src/capture/overlay_model.rs)

In the matched Windows Vulkan 70.13-second cable replay, two chunks finished before stop and the edit control received all 1,344 characters of the combined capture transcript. The same input on release-only chunking independently pasted its 1,339-character capture result. The real-time microphone replays used the same source and were separate captures. The Windows tooltip displayed recognized words underneath the active waveform. Measurements are from one pair of replays. Memory is capture plus worker private resident RAM and worker WDDM dedicated VRAM.

| Windows replay | Release only | Background chunks |
| --- | ---: | ---: |
| Stop to paste | 1.26 s | 1.06 s |
| Mean RAM during recording | 56.8 MiB | 62.0 MiB |
| Mean dedicated VRAM during recording | 187 MiB | 243 MiB |
| Peak dedicated VRAM through paste | 317 MiB | 313 MiB |

The full 605.83-second Windows Vulkan replay finished 24 chunks during capture. It pasted 11,875 characters exactly matching the capture transcript 1.26 seconds after stop. Word error rate was 3.98%, with matching words present in all 36 sections. The prior release-only replay took about six seconds and measured 4.12% WER. The new run averaged 105 MiB of capture plus worker private RAM and 1.19 GiB of dedicated VRAM while recording. Dedicated VRAM peaked at 1.59 GiB. The prior 10-minute replay tracked latency and accuracy. The sustained GPU memory cost makes background processing a better candidate for an opt-in mode than the memory-first default.

Windows formatting, strict lint, 204 focused Rust tests and both static verifiers passed. The Linux shell, capture service and inference worker passed `cargo check --locked --target x86_64-unknown-linux-gnu` with Linux dependency metadata. A native Linux microphone, tooltip and paste replay remains pending because the Linux desktop was unreachable.

The 2026-11-24 Windows installer processed two chunks during a 70.13-second recording. The native tooltip switched from Chunk 1 to Chunk 2 and showed the end of each chunk. Smart Paste inserted the complete transcript 0.62 seconds after release. The Linux capture path shares this logic and passed static checks. Native Linux playback was outside this run.

## Dictation model catalog benchmark

Parakeet.cpp's v0.5.0 results cover all five catalog quantizations for each of the ten built-in Parakeet families. The benchmark used CPU inference with eight threads, one utterance at a time, on the same 100 English LibriSpeech test-clean utterances (901.135 seconds). It used the offline CLI decoder for every family, including Realtime EOU. The published catalog files matched the reported GGUF byte sizes for all 50 family and quantization pairs. [Benchmark methodology](https://github.com/mudler/parakeet.cpp/blob/1bfbebfaaf493866f49597cd3b7901959d395c60/benchmarks/BENCHMARK.md#methodology) · [Offline benchmark command](https://github.com/mudler/parakeet.cpp/blob/1bfbebfaaf493866f49597cd3b7901959d395c60/scripts/benchmark.py#L191-L203) · [Published GGUF files](https://huggingface.co/mudler/parakeet-cpp-gguf/tree/07a08a3a405d20c5434b9a1442f6e6cf571f57ad)

The table shows `manifests.librispeech.ours.<quant>.wer_vs_truth` as a percentage. The runner averages each utterance's word error rate against the human transcript, after normalizing Unicode, capitalization, punctuation and spacing. Lower is better. The scores describe word recognition on this clean English sample. [Pinned per-model results](https://github.com/mudler/parakeet.cpp/tree/1bfbebfaaf493866f49597cd3b7901959d395c60/benchmarks/results) · [EOU result](https://github.com/mudler/parakeet.cpp/blob/1bfbebfaaf493866f49597cd3b7901959d395c60/benchmarks/results/parakeet_realtime_eou_120m-v1.json) · [WER aggregation](https://github.com/mudler/parakeet.cpp/blob/1bfbebfaaf493866f49597cd3b7901959d395c60/scripts/benchmark.py#L222-L247) · [Text normalization](https://github.com/mudler/parakeet.cpp/blob/1bfbebfaaf493866f49597cd3b7901959d395c60/scripts/asr_metrics.py#L22-L37)

| Family | F16 | Q8_0 | Q6_K | Q5_K | Q4_K |
| --- | ---: | ---: | ---: | ---: | ---: |
| TDT-CTC 110M | 3.46% | 3.61% | 3.57% | 3.48% | 2.89% |
| Realtime EOU 120M v1 | 10.92% | 11.31% | 10.92% | 11.09% | 11.06% |
| CTC 0.6B | 4.33% | 4.33% | 4.35% | 4.35% | 4.01% |
| RNN-T 0.6B | 4.41% | 4.41% | 4.40% | 4.38% | 4.25% |
| TDT 0.6B v2 | 3.71% | 3.75% | 3.78% | 3.72% | 3.85% |
| TDT 0.6B v3 | 3.41% | 3.41% | 3.28% | 3.36% | 3.46% |
| CTC 1.1B | 4.75% | 4.75% | 4.61% | 4.59% | 3.46% |
| RNN-T 1.1B | 4.26% | 4.22% | 4.22% | 4.22% | 4.22% |
| TDT 1.1B | 2.51% | 2.44% | 2.44% | 2.44% | 2.40% |
| TDT-CTC 1.1B | 2.55% | 2.51% | 2.56% | 2.56% | 2.52% |

The Lemura Arabic ASR Lite Q8_0 model has no comparable score. Its conversion report checked matching transcripts on four synthetic and three human Arabic clips using CPU and Vulkan. Lemura Labs reports a 25.08% mean WER for the source NeMo checkpoint across six Arabic corpora. [Q8_0 conversion checks](https://huggingface.co/yosef0H4/lemura-arabic-asr-lite-GGUF/blob/9544d4fd4b7e822d986c7995c2f1038167769e5f/README.md#conversion-checks) · [Lemura Labs benchmark](https://huggingface.co/lemuralabs/lemura-arabic-asr-lite/blob/78242ac9ab10da39b45389e70d37cb03b7e7de40/README.md#benchmarks)
