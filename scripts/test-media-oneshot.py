#!/usr/bin/env python3
"""Exercise one-shot media lifetimes with a local provider and an isolated profile."""
import argparse
import io
import json
import os
from pathlib import Path
import queue
import select
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def wav_bytes():
    buffer = io.BytesIO()
    with wave.open(buffer, 'wb') as wav:
        wav.setparams((1, 2, 16000, 0, 'NONE', 'not compressed'))
        wav.writeframes(b'\x00\x10\x00\xf0' * 4000)
    return buffer.getvalue()


def descendants(root):
    result = set()
    pending = [root]
    processes = subprocess.check_output(['ps', '-e', '-o', 'pid=,ppid='], text=True)
    children = {}
    for line in processes.splitlines():
        pid, parent = map(int, line.split())
        children.setdefault(parent, []).append(pid)
    while pending:
        for child in children.get(pending.pop(), []):
            result.add(child)
            pending.append(child)
    return result


def process_live(pid):
    try:
        return (Path('/proc') / str(pid) / 'stat').read_text().rsplit(') ', 1)[1][0] != 'Z'
    except (FileNotFoundError, ProcessLookupError):
        return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bin-dir', type=Path, default=Path('target/debug'))
    bins = parser.parse_args().bin_dir.resolve()
    incoming = queue.Queue()
    audio = wav_bytes()

    class Provider(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            request = self.rfile.read(int(self.headers['Content-Length']))
            complete = threading.Event()
            incoming.put((self.path, request, complete))
            if not complete.wait(timeout=15):
                return
            if b'reject_this' in request:
                code, kind, payload = 400, 'application/json', b'{"error":{"message":"rejected"}}'
            elif self.path == '/v1/audio/speech':
                code, kind, payload = 200, 'audio/wav', audio
            elif self.path == '/v1/audio/transcriptions':
                code, kind, payload = 200, 'application/json', b'{"text":"transcribed"}'
            else:
                code, kind, payload = 200, 'application/json', b'{"choices":[{"message":{"content":"recognized"}}]}'
            self.send_response(code)
            self.send_header('Content-Type', kind)
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass

    with (ThreadingHTTPServer(('127.0.0.1', 0), Provider) as server,
          tempfile.TemporaryDirectory(prefix='smalltalkai-oneshot-') as directory,
          socket.socket(socket.AF_UNIX) as desktop_reader,
          socket.socket() as desktop_capture):
        threading.Thread(target=server.serve_forever, daemon=True).start()
        root = Path(directory)
        profile = root / 'profile'
        reader = profile / 'reader'
        reader.mkdir(parents=True)
        runtime = root / 'runtime'
        runtime.mkdir()
        audio_runtime = root / 'audio-runtime'
        audio_runtime.mkdir(mode=0o700)
        (runtime / 'linux-installation').touch()
        desktop_reader.bind(str(reader / 'simple-tts.sock'))
        desktop_reader.listen()
        desktop_capture.bind(('127.0.0.1', 0))
        desktop_capture.listen()
        state = root / 'data/simple-stt/instances/installed/state'
        state.mkdir(parents=True)
        (state / 'capture-state.json').write_text(json.dumps({
            'protocol': 3, 'pid': os.getpid(), 'started_unix_ms': 1,
            'address': f'127.0.0.1:{desktop_capture.getsockname()[1]}',
        }))
        base = f'http://127.0.0.1:{server.server_port}/v1'
        (reader / 'config.json').write_text(json.dumps({
            'speech': {'provider': {'kind': 'open_ai_compatible', 'base_url': base,
                                    'model': 'saved-tts', 'voice': 'saved-voice', 'format': 'wav'},
                       'fallback_to_local': False},
            'ocr': {'provider': {'kind': 'open_ai_compatible', 'base_url': base,
                                 'model': 'saved-ocr', 'api': 'chat_completions'},
                    'fallback_to_local': False},
        }))
        config = root / 'config.json'
        config.write_text(json.dumps({'speech': {
            'provider': 'remote_openai_compatible',
            'remote_openai_compatible': {'base_url': base, 'model': 'saved-stt', 'fallback_to_local': False},
        }}))
        env = dict(os.environ, HOME=str(root), XDG_CONFIG_HOME=str(root / 'config'),
                   XDG_DATA_HOME=str(root / 'data'), SIMPLE_STT_READER_DATA=str(reader),
                   SIMPLE_STT_CONFIG=str(config), SIMPLE_STT_RUNTIME_ROOT=str(runtime),
                   SIMPLE_TTS_SPEECH_API_KEY='fixture', SIMPLE_TTS_OCR_API_KEY='fixture',
                   SIMPLE_STT_STT_API_KEY='fixture', RUST_LOG='error')
        env.pop('SIMPLE_STT_CLI_WORKSPACE', None)
        played = root / 'played.pcm'
        default_audio = root / 'default.pcm'
        alsa = root / 'alsa.conf'
        alsa.write_text(f'''
</usr/share/alsa/alsa.conf>
pcm.!default {{
    type file
    slave.pcm "null"
    file "{default_audio}"
    format "raw"
}}
pcm.fixture_headphones {{
    type file
    slave.pcm "null"
    file "{played}"
    format "raw"
    hint {{ show on description "Fixture headphones" }}
}}
''')
        env['ALSA_CONFIG_PATH'] = str(alsa)
        env['XDG_RUNTIME_DIR'] = str(audio_runtime)
        env['PIPEWIRE_RUNTIME_DIR'] = str(audio_runtime)
        env['PIPEWIRE_REMOTE'] = 'unavailable-oneshot-test'
        devices = json.loads(subprocess.check_output(
            [str(bins / 'smalltalkai-reader'), 'list-output-devices'], env=env, text=True, timeout=10))
        headphones = next(device['id'] for device in devices if 'fixture_headphones' in device['id'])
        settings = reader / 'config.json'
        selected = json.loads(settings.read_text())
        selected['playback'] = {'output_device': headphones}
        settings.write_text(json.dumps(selected))
        source_audio = root / 'audio.wav'
        source_audio.write_bytes(audio)
        image = Path(__file__).resolve().parents[1] / 'icons/smalltalkai.png'

        def run(args, expected_path, model, status=0, worker=None, stdin=None):
            with subprocess.Popen([str(bins / 'smalltalkai-ctl'), 'media', *map(str, args)], env=env,
                                  stdin=subprocess.PIPE if stdin is not None else subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as command:
                complete = None
                try:
                    if stdin is not None:
                        command.stdin.write(stdin)
                        command.stdin.close()
                        command.stdin = None
                    path, request, complete = incoming.get(timeout=12)
                    assert path == expected_path and model.encode() in request, (path, model)
                    if path.endswith('/speech'):
                        assert json.loads(request)['voice'] == 'saved-voice'
                    owned = descendants(command.pid)
                    if worker:
                        assert any(Path(f'/proc/{pid}/comm').read_text().strip() == worker
                                   for pid in owned if process_live(pid)), owned
                    assert command.poll() is None
                    if status == 130:
                        command.send_signal(signal.SIGINT)
                    else:
                        complete.set()
                    stdout, stderr = command.communicate(timeout=10)
                    assert command.returncode == status, (command.returncode, stderr)
                    deadline = time.monotonic() + 3
                    while any(map(process_live, owned)) and time.monotonic() < deadline:
                        time.sleep(0.02)
                    assert not any(map(process_live, owned)), owned
                    assert not list(profile.glob('.media-*'))
                    assert not select.select([desktop_reader, desktop_capture], [], [], 0)[0]
                    return stdout
                finally:
                    if complete:
                        complete.set()
                    if command.poll() is None:
                        command.send_signal(signal.SIGINT)
                        command.wait(timeout=5)

        target = root / 'result.wav'
        run(['speak', 'read this', '--output', target], '/v1/audio/speech', 'saved-tts',
            worker='smalltalkai-tts')
        with wave.open(str(target), 'rb') as wav:
            assert wav.getframerate() == 24000 and wav.getnframes() > 0
        run(['speak'], '/v1/audio/speech', 'saved-tts', worker='smalltalkai-tts', stdin='play this')
        assert any(played.read_bytes()), 'playback returned without draining nonzero audio'
        assert not default_audio.exists(), 'playback used the default instead of the saved device'
        brief = root / 'brief.txt'
        brief.write_text('The checks passed.', encoding='utf-8')
        submitted = subprocess.run(
            [str(bins / 'smalltalkai-ctl'), 'media', 'speak', '--file', str(brief), '--background'],
            env=dict(env, SIMPLE_STT_READER_DATA=str(root / 'closed-reader')),
            capture_output=True, text=True, timeout=5)
        assert submitted.returncode != 0 and 'running SmallTalkAI Reader' in submitted.stderr, submitted.stderr
        assert incoming.empty() and not list(profile.glob('.media-*'))
        original = target.read_bytes()
        run(['speak', 'keep the old file', '--output', target], '/v1/audio/speech', 'saved-tts',
            status=1, worker='smalltalkai-tts')
        assert target.read_bytes() == original
        assert json.loads(run(['ocr', image, '--json'], '/v1/chat/completions', 'saved-ocr',
                              worker='smalltalkai-ocr')) == {'text': 'recognized'}
        assert run(['transcribe', source_audio], '/v1/audio/transcriptions', 'saved-stt').strip() == 'transcribed'
        failed = root / 'failed.wav'
        run(['speak', 'reject_this', '--output', failed], '/v1/audio/speech', 'saved-tts', status=1,
            worker='smalltalkai-tts')
        assert not failed.exists()
        interrupted = root / 'interrupted.wav'
        run(['speak', 'interrupt this', '--output', interrupted], '/v1/audio/speech', 'saved-tts', status=130,
            worker='smalltalkai-tts')
        assert not interrupted.exists()

        # Use the Rust test worker as the inference binary in a private install.
        install = root / 'local-cli'
        install.mkdir()
        for src, dst in [('smalltalkai-ctl', 'smalltalkai-ctl'),
                         ('smalltalkai_mock_infer', 'smalltalkai-infer')]:
            try:
                os.link(bins / src, install / dst)
            except OSError:
                shutil.copy2(bins / src, install / dst)
        (runtime / 'libparakeet.so').touch()
        (root / 'mock-fast.gguf').touch()
        config.write_text(json.dumps({'speech': {
            'selection_mode': 'single_model', 'single_model_filename': 'mock-fast.gguf',
            'model_dir': str(root), 'runtime_dir': str(runtime), 'inference_device': 'cpu',
        }}))
        local = subprocess.run([str(install / 'smalltalkai-ctl'), 'media', 'transcribe', str(source_audio)],
                               env=env, capture_output=True, text=True, timeout=15)
        assert local.returncode == 0, local.stderr
        assert local.stdout.strip() == 'mock مرحبا 世界 🙂'
        assert not list(profile.glob('.media-*'))
        assert not select.select([desktop_reader, desktop_capture], [], [], 0)[0]
        server.shutdown()
    print('PASS one-shot saved routes, private local STT, playback, WAV export, OCR, errors and Ctrl+C')


if __name__ == '__main__':
    main()
