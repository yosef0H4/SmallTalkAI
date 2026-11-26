# SmallTalkAI

I built SmallTalkAI because I kept closing my dictation app to free up RAM, then reopening it the next time I wanted to type something. I wanted to leave it running, even while playing a game. SmallTalkAI loads each model when you need it. After a while with nothing to do, the worker exits and frees its memory.

SmallTalkAI is open source and runs on Windows and Linux. Speech and screenshots stay on your computer.

![SmallTalk voice typing showcase](gifs/showcase%2C%20with%20actual%20real%20time%20speed.webp)

## Get started

[Download for Windows](https://github.com/yosef0H4/SmallTalkAI/releases/latest) or [install on Fedora or Arch/Omarchy](docs/linux-installation.md). Open Settings from the tray and pick the models you want during setup. Setup uses the runtimes below when it detects a compatible dedicated GPU. Otherwise, it uses CPU for all three features. You can change each runtime during setup or later.

## What I use it for

- **Dictation.** Hold the record shortcut, talk, and let go. SmallTalkAI starts loading the model while you’re talking, then types or pastes into the app you’re already using. On Windows, the default is **Caps Lock + S**. I usually turn off punctuation and enable lowercase for casual messages.
- **Read aloud.** Select part of an article or an agent’s reply and listen while you do something else. You can choose Kokoro or Supertonic 3.
- **Screenshot to text.** Draw a box around text you can’t select, like a line in an image, then paste the extracted text wherever you need it.

Windows uses AutoHotkey for shortcuts and text delivery. You can even stop Caps Lock from toggling caps so it’s free for shortcuts. On Wayland, SmallTalkAI uses the desktop’s Global Shortcuts and Remote Desktop portals.

## Performance with the defaults

On my Windows PC with a Core i5-12600KF and RTX 5080, the whole app used **119 MiB of RAM idle**, including AutoHotkey. With Vulkan dictation loaded, it settled at **200 MiB**. The app briefly reached about **481 MiB** during a first request.

| Feature | Default runtime | First use | Model already loaded | Peak worker RAM | Worker VRAM |
| --- | --- | ---: | ---: | ---: | ---: |
| Dictation · 7.4-second recording | Vulkan | 2.38 s | 64 ms | 395 MiB | 227 MiB |
| Read-aloud · Kokoro | WebGPU | 4.09 s | 605 ms | 469 MiB | 822 MiB |
| Read-aloud · Supertonic 3 | WebGPU | 2.37 s | 225 ms | 308 MiB | 318 MiB |
| OCR · 1080p screenshot | CPU | 415 ms | 203 ms | 153 MiB | 0 MiB |

First use includes loading the model. Dictation starts loading while you talk. Read-aloud starts loading when you ask it to speak. Its timings cover generating the full sample sentence. [See the full benchmark results](docs/performance.md).

If you use read-aloud often and have a compatible NVIDIA GPU, CUDA is an optional way to speed up repeat requests. It uses more memory, and the first request can still take longer. Install it under **Models → Optional accelerators**.

[CLI and agent setup](docs/cli.md#agent-skills) · [Architecture](docs/architecture.md) · [Testing](docs/testing.md) · [GPL-3.0 license](LICENSE) · [Third-party notices](THIRD_PARTY_NOTICES.md)
