#!/usr/bin/env python3
"""Real Settings HTTP regression for schema 10, discovery, and draft-only refresh."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
from urllib.error import HTTPError
from urllib.parse import parse_qs, urlparse
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]


def start_settings_server(binary, config, runtime_root, data_home, **extra_env):
    env = dict(os.environ, SMALLTALKAI_CONFIG=str(config),
               SMALLTALKAI_RUNTIME_ROOT=str(runtime_root), XDG_DATA_HOME=str(data_home))
    env.update(extra_env)
    if os.name != 'nt':
        env.update(XDG_SESSION_TYPE='wayland', WAYLAND_DISPLAY='unavailable-test-display',
                   DBUS_SESSION_BUS_ADDRESS='unix:path=/unavailable-test-bus')
    process = subprocess.Popen([str(binary), '--headless'], cwd=ROOT, env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    line = process.stdout.readline().strip()
    if not line:
        stderr = process.stderr.read()
        raise AssertionError(f'Settings server did not start: {stderr}')
    parsed = urlparse(line)
    origin = f'{parsed.scheme}://{parsed.netloc}'
    token = parse_qs(parsed.fragment)['token'][0]

    def api(path, body=None, auth=True):
        request = Request(origin + path, data=None if body is None else json.dumps(body).encode())
        request.add_header('Origin', origin)
        if auth:
            request.add_header('X-SmallTalkAI-Token', token)
        if body is not None:
            request.add_header('Content-Type', 'application/json')
        with urlopen(request, timeout=10) as response:
            return json.load(response)

    return process, api


def stop_settings_server(process):
    process.terminate()
    process.wait(timeout=5)


def main():
    binary = ROOT / 'target/debug' / ('smalltalkai-settings.exe' if os.name == 'nt' else 'smalltalkai-settings')
    with tempfile.TemporaryDirectory(prefix='smalltalkai-selection-api-') as temporary:
        config = Path(temporary) / 'config.json'
        env = dict(os.environ, SMALLTALKAI_CONFIG=str(config),
                   SMALLTALKAI_RUNTIME_ROOT=str(Path(temporary) / "runtime"),
                   XDG_DATA_HOME=str(Path(temporary) / "data"))
        if os.name != 'nt':
            # Unsupported Wayland cannot inherit the real session's XWayland language.
            env.update(XDG_SESSION_TYPE='wayland', WAYLAND_DISPLAY='unavailable-test-display',
                       DBUS_SESSION_BUS_ADDRESS='unix:path=/unavailable-test-bus')
        process = subprocess.Popen([str(binary), '--headless'], cwd=ROOT, env=env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            parsed = urlparse(process.stdout.readline().strip())
            origin = f'{parsed.scheme}://{parsed.netloc}'
            token = parse_qs(parsed.fragment)['token'][0]

            def api(path, body=None, auth=True):
                request = Request(origin + path, data=None if body is None else json.dumps(body).encode())
                request.add_header('Origin', origin)
                if auth:
                    request.add_header('X-SmallTalkAI-Token', token)
                if body is not None:
                    request.add_header('Content-Type', 'application/json')
                with urlopen(request, timeout=10) as response:
                    return json.load(response)

            state = api('/api/state')
            assert state['config']['general']['ui_language'] == 'auto'
            assert state['ui_localization']['system_locale'] in ('en', 'ar')
            assert api('/api/normalize', {'schema_version': 10, 'general': {'ui_language': 'invalid'}})['config']['general']['ui_language'] == 'auto'
            assert 'model_setup' not in state
            reader_config = api('/api/reader')['config']
            for section in ['speech', 'ocr']:
                assert 'fallback' not in reader_config[section]['selection']
                assert 'uncertain_language' not in reader_config[section]['selection']
            speech = state['config']['speech']
            assert state['config']['schema_version'] == 10
            assert speech['selection_mode'] == 'follow_keyboard'
            assert speech['runtime_dir'] == '' and speech['language_runtimes'] == {}
            assert speech['single_model_filename'] is None and speech['language_models'] == {}
            language_draft = copy.deepcopy(state['config'])
            language_draft['general']['ui_language'] = 'ar'
            api('/api/save', {'config': language_draft, 'expected_hash': state['config_hash']})
            localized = api('/api/state')
            assert localized['ui_localization']['locale'] == 'ar' and localized['ui_localization']['direction'] == 'rtl'
            assert localized['config']['speech'] == speech
            language_draft['general']['ui_language'] = 'auto'
            state = api('/api/save', {'config': language_draft, 'expected_hash': localized['config_hash']})
            before = config.read_bytes()
            try:
                api('/api/keyboard-languages', auth=False)
                raise AssertionError('discovery accepted unauthenticated request')
            except HTTPError as error:
                assert error.code == 400
            languages = api('/api/keyboard-languages')
            assert config.read_bytes() == before, 'refresh wrote configuration'
            if os.name != 'nt':
                assert languages['available'] is False and languages['languages'] == [], languages
                assert 'unavailable' in languages['message'].lower()
            print('PASS authenticated read-only discovery; unsupported Wayland does not use XWayland')
            for version in [7, 8, 9, 11]:
                try:
                    api('/api/normalize', {'schema_version': version})
                    raise AssertionError('unsupported schema accepted')
                except HTTPError as error:
                    assert error.code == 400
                    assert 'unsupported config schema_version' in error.read().decode()
            assert config.read_bytes() == before, 'unsupported schema changed saved configuration'
            normalized_obsolete = api('/api/normalize', {'schema_version': 10, 'speech': {
                'language_mode': 'arabic', 'english_model_filename': 'old-en.gguf',
                'arabic_model_filename': 'old-ar.gguf'}})['config']['speech']
            assert normalized_obsolete['single_model_filename'] is None
            assert normalized_obsolete['language_models'] == {}
            draft = copy.deepcopy(state['config'])
            draft['speech'].update(selection_mode='follow_keyboard', single_model_filename='missing.gguf',
                                   language_models={'en': None, 'ar': 'tdt_ctc-110m-q8_0.gguf',
                                                    'xkb:unknown:variant': 'missing.gguf'})
            normalized = api('/api/normalize', draft)['config']
            assert normalized['speech'] == draft['speech'], 'metadata or missing files changed choice'
            assert config.read_bytes() == before, 'import preview wrote configuration'
            saved = api('/api/save', {'config': normalized, 'expected_hash': state['config_hash']})
            assert saved['config']['speech'] == draft['speech']
            assert json.loads(config.read_text())['speech'] == draft['speech']
            before = config.read_bytes()
            api('/api/keyboard-languages')
            assert config.read_bytes() == before
            defaults = api('/api/defaults')['config']['speech']
            assert defaults['single_model_filename'] is None and defaults['language_models'] == {}
            assert config.read_bytes() == before, 'reset preview wrote configuration'
            assert 'model_setup' not in api('/api/state')
            current=api('/api/state')['config']['speech']
            assert current['language_models']==normalized['speech']['language_models']
            print('PASS empty defaults, no onboarding side effects, explicit routing, None/import/reset/save')
        finally:
            process.terminate()
            process.wait(timeout=5)


def test_first_run_preserves_user_choice():
    binary = ROOT / 'target/debug' / ('smalltalkai-settings.exe' if os.name == 'nt' else 'smalltalkai-settings')
    with tempfile.TemporaryDirectory(prefix='smalltalkai-first-run-api-') as temporary:
        folder = Path(temporary)
        config = folder / 'config.json'
        no_driver = str(folder / 'missing-vulkan-driver.json')
        process, api = start_settings_server(
            binary, config, folder / 'runtime', folder / 'data',
            SMALLTALKAI_READER_DATA=str(folder / 'reader'),
            VK_DRIVER_FILES=no_driver, VK_ICD_FILENAMES=no_driver,
        )
        try:
            assert api('/api/model-setup')['step'] == 0
            state = api('/api/state')
            assert state['config']['speech']['inference_device'] == 'cpu'
            reader = api('/api/reader')['config']
            assert reader['ocr']['selection']['single']['device'] == 'cpu'
            assert reader['speech']['selection']['single']['device'] == 'cpu'
            state['config']['speech']['inference_device'] = 'gpu'
            api('/api/save', {'config': state['config'], 'expected_hash': state['config_hash']})
            api('/api/model-setup')
            assert api('/api/state')['config']['speech']['inference_device'] == 'gpu'
            assert (folder / 'model-setup.json').is_file()
            print('PASS first-run CPU defaults with no driver and preservation of an explicit user choice')
        finally:
            stop_settings_server(process)


if __name__ == '__main__':
    main()
    test_first_run_preserves_user_choice()
