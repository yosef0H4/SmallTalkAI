#!/usr/bin/env python3
"""Measure Windows workloads with the installed inference workers."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time
import wave

from PIL import Image, ImageDraw, ImageFont
import psutil

ROOT = Path(__file__).resolve().parents[1]
SPEECH = importlib.util.spec_from_file_location(
    "speech_bench", ROOT / "scripts/bench-speech-devices.py"
)
speech = importlib.util.module_from_spec(SPEECH)
SPEECH.loader.exec_module(speech)
MIB = 1024 ** 2
VOICE_TEXT = "SmallTalk reads this paragraph aloud while I keep working in another window."


def dedicated_mib(pids):
    """Read Windows' per-process dedicated GPU byte counters (WDDM)."""
    ids = ",".join(map(str, pids))
    command = (
        f"$measureIds=@({ids}); "
        "$items=(Get-Counter '\\GPU Process Memory(*)\\Dedicated Usage' "
        "-MaxSamples 1 -ErrorAction Stop).CounterSamples; "
        "($items | Where-Object { $_.InstanceName -match '^pid_(\\d+)_' "
        "-and [int]$Matches[1] -in $measureIds } | Measure-Object CookedValue -Sum).Sum"
    )
    result = subprocess.run(
        ["powershell.exe", "-NoProfile", "-Command", command],
        capture_output=True, text=True, timeout=12, check=True,
    )
    return float(result.stdout.strip() or 0) / MIB


def make_images(folder):
    images = {}
    for width, height in [(640, 360), (1920, 1080)]:
        image = Image.new("RGB", (width, height), "white")
        scale = width // 640
        font = ImageFont.truetype("C:/Windows/Fonts/segoeui.ttf", 26 * scale)
        canvas = ImageDraw.Draw(image)
        for line, text in enumerate((
            "INVOICE NO 2026", "SmallTalk reads screenshots.",
            "Total amount 123.45", "Payment received",
        )):
            canvas.text((36 * scale, (42 + line * 63) * scale), text, fill="black", font=font)
        path = folder / f"{width}x{height}.png"
        image.save(path)
        images[path.stem] = path
    return images


def open_worker(case, bin_dir, log):
    feature, _, backend, model, runtime = case[:5]
    binary = {"stt": "infer", "tts": "tts", "ocr": "ocr"}[feature]
    exe = bin_dir / f"smalltalkai-{binary}.exe"
    if feature == "stt":
        command = [exe, "--runtime-dir", runtime, "--model-path", model, "--language",
                   "en", "--inference-device", "cpu" if backend == "CPU" else "gpu",
                   "--idle-timeout-secs", "120", "--log-path", log.with_suffix(".component.log")]
    else:
        device = "cpu" if backend == "CPU" else ("cuda" if feature == "tts" and backend == "CUDA" else "gpu")
        command = [exe, "--runtime-dir", runtime, "--model-dir", model, "--device", device]
        if feature == "tts":
            provider = "local_kokoro" if case[1] == "Kokoro" else "local_supertonic"
            command += ["--provider", provider, "--threads", "2"]
        if backend == "WebGPU":
            command.append("--webgpu")
    env = dict(os.environ)
    env.pop("PARAKEET_DEVICE", None)
    if feature == "ocr":
        env["SMALLTALKAI_READER_CAPTURE_DIR"] = str(case[5].parent)
    start = time.perf_counter()
    worker = speech.Worker(list(map(str, command)), env, log, "stt" if feature == "stt" else "reader")
    try:
        if feature == "stt":
            worker.stt_send(1)
            assert worker.receive(20)[0] == 2
        else:
            worker.reader_send({"type": "hello", "protocol": 1, "incarnation": "benchmark"})
            assert worker.receive(20)[1]["type"] == "ready"
        return worker, start
    except BaseException:
        worker.close()
        raise


def request(worker, case, sequence):
    feature, model_name, backend = case[:3]
    payload = case[5]
    if feature == "stt":
        return 1000 * speech.stt_case(worker, payload, sequence, 30), None
    if feature == "tts":
        device = {"CPU": "cpu", "CUDA": "cuda", "WebGPU": "gpu"}[backend]
        voice = "af_heart" if model_name == "Kokoro" else "sid:0"
        elapsed, generated = speech.tts_case(worker, VOICE_TEXT, voice, "en", device, sequence, 35)
        return 1000 * elapsed, round(generated, 3)
    job = str(sequence)
    start = time.perf_counter()
    worker.reader_send({"type": "recognize", "job_id": job, "generation": 1,
                        "image_path": str(payload), "language": "eng"})
    text = None
    while True:
        _, event, _ = worker.receive(max(.01, 30 - (time.perf_counter() - start)))
        if event["type"] == "error":
            raise RuntimeError(str(event))
        if event["type"] == "text":
            text = event["text"]
        if event["type"] == "finished":
            break
    if not text or not re.search(r"invoice|123|payment", text, re.I):
        raise RuntimeError("OCR did not recognize the fixture text")
    return 1000 * (time.perf_counter() - start), text


def cpu_seconds(process):
    times = process.cpu_times()
    return times.user + times.system


def run_utilization(case, args, log_dir, label):
    worker, start = open_worker(case, args.bin_dir, log_dir / f"{label}-utilization.log")
    process = psutil.Process(worker.child.pid)
    sample_command = (
        f"$workerPattern='^pid_{process.pid}_'; "
        "Get-Counter '\\GPU Engine(*)\\Utilization Percentage' -SampleInterval 1 "
        "-MaxSamples 3 -ErrorAction Stop | ForEach-Object { "
        "$values=$_.CounterSamples | Where-Object { $_.InstanceName -match $workerPattern }; "
        "[double](($values | Measure-Object CookedValue -Maximum).Maximum) }"
    )
    counter = None
    try:
        request(worker, case, 1)
        cold_wall = time.perf_counter() - start
        cold_cpu = cpu_seconds(process)
        counter = subprocess.Popen(
            ["powershell.exe", "-NoProfile", "-Command", sample_command],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        start = time.perf_counter()
        warm_cpu_before = cpu_seconds(process)
        requests = 0
        while requests < 3 or (counter.poll() is None and time.perf_counter() - start < 8):
            requests += 1
            request(worker, case, requests + 1)
        warm_cpu = cpu_seconds(process) - warm_cpu_before
        warm_wall = time.perf_counter() - start
        stdout, stderr = counter.communicate(timeout=3)
        if counter.returncode:
            raise RuntimeError(f"Windows GPU utilization counter failed: {stderr.strip()}")
        samples = [float(value) for value in stdout.splitlines() if value.strip()][1:]
        if len(samples) != 2:
            raise RuntimeError(f"Expected two full GPU counter samples, got {len(samples)}")
        logical_cpus = psutil.cpu_count() or 1
        result = {
            "feature": case[0], "model": case[1], "runtime": case[2],
            "case": case[5].stem if case[0] == "ocr" else None,
            "cold_cpu_ms": round(1000 * cold_cpu, 1),
            "cold_cpu_machine_percent": round(100 * cold_cpu / cold_wall / logical_cpus, 1),
            "warm_cpu_ms_per_request": round(1000 * warm_cpu / requests, 1),
            "warm_cpu_machine_percent": round(100 * warm_cpu / warm_wall / logical_cpus, 1),
            "warm_gpu_busiest_engine_percent": round(statistics.mean(samples), 1),
            "warm_requests": requests,
        }
        print(f"{label}: warm CPU {result['warm_cpu_ms_per_request']} ms per request, "
              f"{result['warm_cpu_machine_percent']}% CPU, "
              f"{result['warm_gpu_busiest_engine_percent']}% GPU", flush=True)
        return result
    finally:
        if counter is not None and counter.poll() is None:
            counter.kill()
            counter.communicate()
        worker.close()


def run_case(case, args, log_dir):
    feature, name, backend, _, _, payload = case
    label = f"{feature}-{name}-{backend}" + (f"-{payload.stem}" if feature == "ocr" else "")
    if args.utilization_only:
        return run_utilization(case, args, log_dir, label)
    result = {"feature": feature, "model": name, "runtime": backend, "case": payload.stem if feature == "ocr" else None,
              "cold_ms": [], "warm_ms": [], "worker_peak_ram_mib": [], "worker_loaded_vram_mib": None}
    for i in range(args.cold):
        worker, start = open_worker(case, args.bin_dir, log_dir / f"{label}-{i}.log")
        try:
            _, output = request(worker, case, 1)
            result["cold_ms"].append(round((time.perf_counter() - start) * 1000, 2))
            if i == args.cold - 1:
                for seq in range(2, 2 + args.warm):
                    elapsed, output = request(worker, case, seq)
                    result["warm_ms"].append(round(elapsed, 2))
                if feature == "tts":
                    result["generated_audio_seconds"] = output
                if feature == "ocr":
                    result["recognized_text"] = output
                result["worker_loaded_vram_mib"] = round(dedicated_mib([worker.child.pid]), 2)
            memory = psutil.Process(worker.child.pid).memory_info()
            result["worker_peak_ram_mib"].append(round(memory.peak_wset / MIB, 2))
        finally:
            worker.close()
    result["cold_median_ms"] = round(statistics.median(result["cold_ms"]), 2)
    result["warm_median_ms"] = round(statistics.median(result["warm_ms"]), 2)
    print(f"{label}: cold {result['cold_median_ms']} ms, warm {result['warm_median_ms']} ms, "
          f"RAM {max(result['worker_peak_ram_mib'])} MiB, VRAM {result['worker_loaded_vram_mib']} MiB",
          flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--reader-assets", type=Path, required=True)
    parser.add_argument("--stt-cuda", type=Path, required=True)
    parser.add_argument("--reader-cuda", type=Path, required=True)
    parser.add_argument("--stt-model", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cold", type=int, default=3)
    parser.add_argument("--warm", type=int, default=5)
    parser.add_argument("--utilization-only", action="store_true",
                        help="Measure worker CPU time and GPU-engine activity during repeat requests.")
    parser.add_argument("--only", action="append", default=[])
    args = parser.parse_args()
    args.bin_dir = args.bin_dir.resolve()
    args.reader_assets = args.reader_assets.resolve()
    args.stt_cuda = args.stt_cuda.resolve()
    args.stt_model = args.stt_model.resolve()
    args.output = args.output.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    log_dir = args.output.parent / (args.output.stem + "-logs")
    log_dir.mkdir(exist_ok=True)
    bundled = args.bin_dir / "bundled-runtimes"
    reader_runtimes = {"CPU": bundled / "sherpa-cpu-windows",
                       "CUDA": args.reader_cuda.resolve(),
                       "WebGPU": bundled / "paddle-webgpu-windows"}
    with tempfile.TemporaryDirectory(prefix="smalltalk-benchmark-") as tmp:
        images = make_images(Path(tmp))
        self_test = ROOT / "fixtures/parakeet-smoke.wav"
        with wave.open(str(self_test), "rb") as audio:
            audio_seconds = audio.getnframes() / audio.getframerate()
        pcm = speech.pcm_source(self_test)
        report = {"cold_runs": 1 if args.utilization_only else args.cold,
                  "warm_runs": "back-to-back" if args.utilization_only else args.warm,
                  "dictation_input_seconds": audio_seconds,
                  "tts_text": VOICE_TEXT, "rows": [], "unavailable": []}
        vulkan = bundled / "parakeet-vulkan-windows"
        cases = [("stt", "Parakeet 110M Q8", backend, args.stt_model, runtime, pcm)
                 for backend, runtime in (("CPU", vulkan), ("Vulkan", vulkan), ("CUDA", args.stt_cuda))]
        cases += [("ocr", "PaddleOCR Arabic + English", backend, args.reader_assets / "paddle-ar-en", runtime, image)
                  for backend, runtime in reader_runtimes.items() for image in images.values()]
        cases += [("tts", name, backend, args.reader_assets / folder, runtime, None)
                  for backend, runtime in reader_runtimes.items()
                  for name, folder in (("Kokoro", "kokoro"), ("Supertonic 3", "supertonic3"))]
        for case in cases:
            if args.only and not any(tag in f"{case[0]}-{case[1]}-{case[2]}" for tag in args.only):
                continue
            try:
                report["rows"].append(run_case(case, args, log_dir))
            except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
                print(f"{case[:3]} failed: {error}", flush=True)
                report["unavailable"].append({"feature": case[0], "model": case[1],
                    "runtime": case[2], "case": case[5].stem if case[0] == "ocr" else None, "error": str(error)})
            args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")


if __name__ == "__main__":
    main()
