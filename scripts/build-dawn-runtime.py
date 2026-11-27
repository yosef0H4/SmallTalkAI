"""Build and stage the native Dawn-enabled ONNX Runtime used by OCR and speech."""
import argparse
import ctypes
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

REVISION = "41d4d61f14b9c1dd0b2ccda1e9b85dd415fd0496"
TREE = "swh:1:dir:b4e80771381c5cc9bf36853527686c12ad531f82"
ARCHIVE_HASH = "676f0a7280ceb66994f7ee6666f5d226df2e4929a1e41215bef9824c0d96029d"
ARCHIVE_URL = f"https://archive.softwareheritage.org/api/1/vault/flat/{TREE}/raw/"
ADAPTER_QUERY = "SmallTalkAIWebGpuVisitAdaptersV1"
PATCH = Path(__file__).resolve().with_name("onnxruntime-webgpu-adapters.patch")


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def prepare_source(work, patch_id):
    source = work / f"src-{patch_id}"
    if source.is_dir():
        return source
    archive = work / f"{REVISION}.tar.gz"
    if not archive.is_file() or digest(archive) != ARCHIVE_HASH:
        with tempfile.TemporaryDirectory(dir=work) as temporary:
            download = Path(temporary) / "onnxruntime.tar.gz"
            with urllib.request.urlopen(ARCHIVE_URL, timeout=60) as response, download.open("wb") as out:
                shutil.copyfileobj(response, out)
            if digest(download) != ARCHIVE_HASH:
                raise RuntimeError("The archived ONNX Runtime source does not match the pinned SHA-256")
            download.replace(archive)
    with tempfile.TemporaryDirectory(dir=work) as temporary:
        staging = Path(temporary) / "src"
        staging.mkdir()
        with tarfile.open(archive) as contents:
            for member in contents:
                parent, _, name = member.name.partition("/")
                if parent != TREE:
                    raise RuntimeError(f"Unexpected ONNX Runtime archive entry: {member.name}")
                if name:
                    # Strip Software Heritage's colon-containing directory name on Windows.
                    contents.extract(member.replace(name=name), staging, filter="data")
        subprocess.run(["git", "apply", str(PATCH)], cwd=staging, check=True)
        staging.rename(source)
    return source


def build_runtime(work, jobs):
    patch_id = digest(PATCH)[:16]
    source = prepare_source(work, patch_id)
    build = work / f"build-{patch_id}"
    windows = os.name == "nt"
    generator = "Visual Studio 17 2022" if windows else "Ninja"
    backend = "D3D12" if windows else "VULKAN"
    subprocess.run([
        sys.executable, str(source / "tools/ci_build/build.py"),
        "--build_dir", str(build), "--config", "Release", "--cmake_generator", generator,
        "--parallel", str(jobs), "--target", "onnxruntime", "--build_shared_lib",
        "--use_webgpu", "--skip_tests", "--skip_submodule_sync",
        "--cmake_extra_defines", "onnxruntime_BUILD_UNIT_TESTS=OFF",
        "CMAKE_INTERPROCEDURAL_OPTIMIZATION=OFF", f"onnxruntime_ENABLE_DAWN_BACKEND_{backend}=ON",
    ], cwd=source, check=True)
    release = build / "Release"
    return release / "Release/onnxruntime.dll" if windows else release / "libonnxruntime.so.1"


def stage_runtime(bundle, library):
    if not library.is_file():
        raise RuntimeError(f"Missing built Dawn runtime: {library}")
    library_dir = bundle / "lib"
    if os.name == "nt":
        # ORT's D3D12 provider and the staged runtime use the pinned DXCompiler build.
        with os.add_dll_directory(str(library_dir)):
            runtime = ctypes.WinDLL(str(library))
            getattr(runtime, ADAPTER_QUERY)
            del runtime
        destination = library_dir / "onnxruntime.dll"
    else:
        symbols = subprocess.check_output(["nm", "-D", "--defined-only", library], text=True)
        dynamic = subprocess.check_output(["readelf", "-d", library], text=True)
        if not any(line.split()[-1].split("@", 1)[0] == ADAPTER_QUERY
                   for line in symbols.splitlines() if line.strip()) or "[libonnxruntime.so.1]" not in dynamic:
            raise RuntimeError("The ONNX Runtime library is missing the Dawn export or expected SONAME")
        destination = library_dir / "libonnxruntime.so"
    shutil.copy2(library, destination)
    (bundle / ".verified").write_text(f"source:onnxruntime:{REVISION}", encoding="ascii")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--library", type=Path, help="Reuse an already built, patched library")
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    library = args.library.resolve() if args.library else build_runtime(work, args.jobs)
    stage_runtime(args.bundle.resolve(), library)


if __name__ == "__main__":
    main()
