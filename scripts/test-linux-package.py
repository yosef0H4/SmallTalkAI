#!/usr/bin/env python3
"""Installed-package smoke check; no microphone, desktop, or clipboard access."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import threading
import time
import wave
from urllib.parse import parse_qs, urlparse
from urllib.request import Request, urlopen
from urllib.error import HTTPError


def main():
    root = Path('/usr/lib/smalltalkai')
    assert os.geteuid() != 0, 'Run the installed smoke as an ordinary user'
    assert not os.access(root, os.W_OK), 'Package directory must be root-owned'
    names = ['smalltalkai-linux', 'smalltalkai-capture', 'smalltalkai-infer',
             'smalltalkai-ctl', 'smalltalkai-settings', 'smalltalkai-settings-window', 'smalltalkai-paste',
             'smalltalkai-reader', 'smalltalkai-readerctl', 'smalltalkai-tts', 'smalltalkai-ocr', 'smalltalkai-reader-hud']
    for name in names:
        executable = root / name
        assert executable.is_file() and os.access(executable, os.X_OK), name
        assert not os.access(executable, os.W_OK), name
        result = subprocess.run(['ldd', str(executable)], capture_output=True, text=True, check=True)
        assert 'not found' not in result.stdout, result.stdout
    bundled = root / 'bundled-runtimes'
    assert {path.name for path in bundled.iterdir()} == {'parakeet-vulkan', 'sherpa-cpu'}
    assert all((path / '.verified').is_file() for path in bundled.iterdir())
    assert not list(bundled.rglob('*.gguf')) and not list(bundled.rglob('*.onnx'))
    adapter = root / 'sherpa-webgpu' / 'libsherpa-onnx-c-api.so'
    assert adapter.is_file() and not os.access(adapter, os.W_OK)
    imports = subprocess.check_output(['readelf', '-d', str(adapter)], text=True)
    assert '[libonnxruntime.so.1]' in imports and 'RUNPATH' not in imports, imports
    assert not list(bundled.glob('*webgpu*')), 'Linux WebGPU runtime must remain an optional download'
    assert (root / 'downloads.json').is_file()
    assert (root / 'lid.176.ftz').stat().st_size == 938013
    for name in ('fixtures/parakeet-smoke.wav', 'fixtures/asr/arabic-sa.wav'):
        with wave.open(str(root / name), 'rb') as audio:
            assert audio.getnframes() > 0 and audio.getnchannels() == 1, name
            assert audio.getframerate() == 16000, name
    symbols = subprocess.check_output(['nm', '-D', str(root / 'smalltalkai-paste')], text=True)
    for feature in ['atspi_', 'g_dbus_']:
        assert feature in symbols, f'Paste helper missing {feature}'
    assert "XTestFakeKeyEvent" not in symbols, "Legacy XTest injection must not ship"
    for installed in ['/usr/bin/smalltalkai', '/usr/lib/systemd/user/smalltalkai-linux.service',
                      '/usr/lib/systemd/user/smalltalkai-server.service',
                      '/usr/share/applications/io.github.yosef0H4.smalltalkai.desktop',
                      '/usr/share/applications/smalltalkai-reader.desktop']:
        assert Path(installed).is_file(), installed
    assert Path('/usr/bin/smalltalkai-ctl').samefile(root / 'smalltalkai-ctl')
    assert not Path('/usr/share/applications/smalltalkai-settings.desktop').exists()
    launcher = Path('/usr/share/applications/io.github.yosef0H4.smalltalkai.desktop').read_text()
    assert 'Name=SmallTalkAI\n' in launcher and 'Exec=smalltalkai launch\n' in launcher
    for size in (16, 22, 24, 32, 48, 64, 128, 256):
        icon = Path(f'/usr/share/icons/hicolor/{size}x{size}/apps/smalltalkai.png')
        with icon.open('rb') as stream:
            header = stream.read(24)
        assert header.startswith(b'\x89PNG\r\n\x1a\n')
        assert int.from_bytes(header[16:20], 'big') == int.from_bytes(header[20:24], 'big') == size
    kwin_grant = Path('/usr/share/applications/smalltalkai-reader.desktop').read_text()
    assert 'Exec=/usr/lib/smalltalkai/smalltalkai-reader' in kwin_grant
    assert 'X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2' in kwin_grant
    assert (root / 'linux-installation').is_file()
    with tempfile.TemporaryDirectory(prefix='smalltalkai-package-') as temporary:
        home = Path(temporary)
        env = dict(os.environ, HOME=temporary, XDG_CONFIG_HOME=str(home / 'config'),
                   XDG_DATA_HOME=str(home / 'data'), XDG_CACHE_HOME=str(home / 'cache'),
                   XDG_RUNTIME_DIR=str(home / 'run'), XDG_SESSION_TYPE='wayland',
                   WAYLAND_DISPLAY='unavailable-package-test-display',
                   DBUS_SESSION_BUS_ADDRESS='unix:path=/unavailable-package-test-bus')
        for name in ['SMALLTALKAI_RUNTIME_ROOT', 'SMALLTALKAI_CONFIG', 'SMALLTALKAI_BINDIR', 'DISPLAY']:
            env.pop(name, None)
        (home / 'run').mkdir(mode=0o700)
        subprocess.run(['/usr/bin/smalltalkai', '--help'], cwd=home, env=env,
                       check=True, stdout=subprocess.DEVNULL, timeout=10)
        # ExecStop must also succeed after a tray quit has removed capture state.
        subprocess.run(['/usr/bin/smalltalkai', 'shutdown'], cwd=home, env=env,
                       check=True, timeout=10)
        process = subprocess.Popen([str(root / 'smalltalkai-settings'), '--headless'],
                                   cwd=home, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True)
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                assert selector.select(timeout=30), 'Settings did not announce its URL'
            parsed = urlparse(process.stdout.readline().strip())
            origin = f'{parsed.scheme}://{parsed.netloc}'
            with urlopen(f'{origin}/smalltalkai.png', timeout=10) as icon:
                assert icon.headers.get_content_type() == 'image/png'
                launcher_icon = icon.read()
                assert launcher_icon == Path('/usr/share/icons/hicolor/64x64/apps/smalltalkai.png').read_bytes()
            with urlopen(f'{origin}/smalltalkai-titlebar.png', timeout=10) as icon:
                assert icon.headers.get_content_type() == 'image/png'
                titlebar_icon = icon.read()
                assert titlebar_icon[:8] == launcher_icon[:8]
                assert titlebar_icon[20:24] == launcher_icon[20:24]
                assert 0 < int.from_bytes(titlebar_icon[16:20], 'big') < 64
            token = parse_qs(parsed.fragment)['token'][0]
            request = Request(origin + '/api/state', headers={
                'Origin': origin, 'X-SmallTalkAI-Token': token})
            with urlopen(request, timeout=20) as response:
                state = json.load(response)
            assert state['config_path'] == str(home / 'config/smalltalkai/instances/installed/config.json')
            vulkan = str(bundled / 'parakeet-vulkan')
            assert state['config']['speech']['runtime_dir'] == vulkan
            assert state['resolved_runtime_dir'] == vulkan
            assert state['resolved_model_dir'] == str(home / 'data/smalltalkai/models')
            assert state['runtime_ready']
            assert not state['service_online']
            assert state['linux_automation']['recommended'] == 'portal', 'installed input must use the portal'
            assert (root / 'smalltalkai-paste').is_file(), 'read-only desktop helper missing'
            assert state['config']['speech']['single_model_filename'] is None
            assert 'model_setup' not in state
            assert state['config']['speech']['selection_mode']=='follow_keyboard'
            assert state['config']['speech']['language_models']=={}
            assert state['config']['speech']['language_runtimes']=={}
            (home / 'data/smalltalkai/models').mkdir(parents=True, exist_ok=True)
            (home / 'data/smalltalkai/models/write-test').write_text('user-writable')
            # A stopped portal shell must return an error, never report success
            # or launch a real desktop settings application in this isolated test.
            action = Request(origin + '/api/platform-action',
                             data=json.dumps({'action': 'configure_shortcuts'}).encode(),
                             headers={'Origin': origin, 'X-SmallTalkAI-Token': token,
                                      'Content-Type': 'application/json'})
            try:
                urlopen(action, timeout=15).close()
                raise AssertionError('Stopped shortcut shell incorrectly reported success')
            except HTTPError as error:
                assert error.code >= 400
                assert b'not running' in error.read(), 'Missing actionable shortcut error'
            # Exercise the real Settings -> shell request/result transport with
            # a fake portal responder. No session bus, keyboard or host UI is used.
            shell_data = home / 'data/smalltalkai/instances/installed'
            (shell_data / 'linux-daemon.pid').write_text(str(os.getpid()))
            request_file = shell_data / 'linux-configure-shortcuts.request'
            result_file = shell_data / 'linux-configure-shortcuts.result'
            for reply, expected_status in [('portal shortcut configuration opened', 200),
                                            ('error: isolated portal failure', 400)]:
                failures = []
                def respond():
                    deadline = time.monotonic() + 10
                    while not request_file.exists() and time.monotonic() < deadline:
                        time.sleep(0.02)
                    if not request_file.exists():
                        failures.append('shell never requested portal configuration')
                        return
                    request_file.unlink()
                    result_file.write_text(reply)
                responder = threading.Thread(target=respond)
                responder.start()
                try:
                    with urlopen(action, timeout=15) as response:
                        assert response.status == expected_status
                except HTTPError as error:
                    assert error.code == expected_status
                    assert b'isolated portal failure' in error.read()
                finally:
                    responder.join(timeout=12)
                assert not failures and not responder.is_alive(), failures
                assert not result_file.exists(), 'portal result was not consumed'
            (shell_data / 'linux-daemon.pid').unlink()
            with urlopen(origin, timeout=10) as response:
                assert b'<html' in response.read().lower()
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
    print('PASS installed binaries, dependencies, ABI 6, full paste helper, embedded UI, stable profile and writable models')


if __name__ == '__main__':
    main()
