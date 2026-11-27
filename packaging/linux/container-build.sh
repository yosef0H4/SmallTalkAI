#!/usr/bin/env bash
set -euo pipefail

distro="${1:?expected fedora or arch}"
if [[ -n "${SMALLTALKAI_BUILD_CORE:-}" ]]; then
  build_core="$SMALLTALKAI_BUILD_CORE"
  unset SMALLTALKAI_BUILD_CORE
  exec taskset -c "$build_core" ionice -c3 nice -n 19 bash "$0" "$@"
fi
source_dir=/source
work=/work/build
python3 -c 'import shutil; [shutil.rmtree("/work/build/" + name, ignore_errors=True) for name in ("src", "out", "stage", "rpmbuild", "arch-package")]'
mkdir -p "$work/src" "$work/out"
tar -C "$source_dir" -cf - . | tar -C "$work/src" -xf -
cd "$work/src"
python3 scripts/prepare-linux-package-source.py "$work/src"

npm --prefix web/settings ci
npm --prefix web/settings run build
python3 scripts/verify-settings-frontend.py
for feature in x11 xtst gio-2.0 atspi-2; do
  pkg-config --exists "$feature" || { echo "missing required helper feature: $feature" >&2; exit 1; }
done
python3 scripts/build-smalltalkai-paste.py
test -x resources/bin/smalltalkai-paste
nm -D resources/bin/smalltalkai-paste > "$work/smalltalkai-paste.symbols"
grep -q atspi_ "$work/smalltalkai-paste.symbols"
grep -q g_dbus_ "$work/smalltalkai-paste.symbols"
! grep -q XTestFakeKeyEvent "$work/smalltalkai-paste.symbols"
cargo +1.99.0 build --release --locked --jobs "${CARGO_BUILD_JOBS:-2}" \
  -p smalltalkai -p smalltalkai-settings-window \
  --bin smalltalkai-linux --bin smalltalkai-capture --bin smalltalkai-infer \
  --bin smalltalkai-ctl --bin smalltalkai-settings --bin smalltalkai-settings-window --bin smalltalkai-reader-hud
cargo +1.99.0 build --release --locked --jobs "${CARGO_BUILD_JOBS:-2}" \
  -p smalltalkai-reader -p smalltalkai-readerctl -p smalltalkai-tts -p smalltalkai-ocr

stage="$work/stage"
app="$stage/usr/lib/smalltalkai"
mkdir -p "$stage/usr/bin" "$app" \
  "$stage/usr/share/applications" \
  "$stage/usr/share/icons/hicolor" \
  "$stage/usr/lib/systemd/user" "$stage/usr/share/licenses/smalltalkai"
install -m 0755 packaging/linux/smalltalkai "$stage/usr/bin/smalltalkai"
ln -s ../lib/smalltalkai/smalltalkai-ctl "$stage/usr/bin/smalltalkai-ctl"
for binary in smalltalkai-linux smalltalkai-capture smalltalkai-infer smalltalkai-ctl smalltalkai-settings smalltalkai-settings-window smalltalkai-reader smalltalkai-readerctl smalltalkai-tts smalltalkai-ocr smalltalkai-reader-hud; do
  install -m 0755 "$CARGO_TARGET_DIR/release/$binary" "$app/$binary"
done
"$app/smalltalkai-ctl" assets stage-bundled "$app/bundled-runtimes"
dawn_args=()
if [[ -f /patched-onnxruntime.so ]]; then
  dawn_args=(--library /patched-onnxruntime.so)
fi
python3 scripts/build-dawn-runtime.py --work /work/dawn-runtime \
  --bundle "$app/bundled-runtimes/paddle-webgpu-linux" --jobs "${CARGO_BUILD_JOBS:-2}" "${dawn_args[@]}"
bash scripts/build-sherpa-webgpu.sh "$app/smalltalkai-ctl" "$app/sherpa-webgpu"
install -m 0755 resources/bin/smalltalkai-paste "$app/smalltalkai-paste"
install -D -m 0644 fixtures/parakeet-smoke.wav "$app/fixtures/parakeet-smoke.wav"
install -D -m 0644 fixtures/asr/arabic-sa.wav "$app/fixtures/asr/arabic-sa.wav"
install -m 0644 packaging/linux/io.github.yosef0H4.smalltalkai.desktop "$stage/usr/share/applications/"
install -m 0644 packaging/linux/smalltalkai-reader.desktop "$stage/usr/share/applications/"
cp -a icons/hicolor/. "$stage/usr/share/icons/hicolor/"
install -m 0644 packaging/linux/smalltalkai-linux.service "$stage/usr/lib/systemd/user/"
install -m 0644 packaging/linux/smalltalkai-server.service "$stage/usr/lib/systemd/user/"
install -m 0644 LICENSE "$stage/usr/share/licenses/smalltalkai/"
install -m 0644 THIRD_PARTY_NOTICES.md "$stage/usr/share/licenses/smalltalkai/"
install -m 0644 assets/catalog/downloads.json "$app/downloads.json"
install -m 0644 assets/language/lid.176.ftz "$app/lid.176.ftz"
install -m 0644 assets/language/LICENSE-CC-BY-SA-3.0.txt "$stage/usr/share/licenses/smalltalkai/"
install -m 0644 assets/language/ATTRIBUTION.md "$stage/usr/share/licenses/smalltalkai/fastText-ATTRIBUTION.md"
install -m 0644 assets/fonts/LICENSE-JetBrainsMono.txt "$stage/usr/share/licenses/smalltalkai/OFL-JetBrainsMono.txt"
install -m 0644 assets/fonts/LICENSE-NotoSansArabic.txt "$stage/usr/share/licenses/smalltalkai/OFL-NotoSansArabic.txt"
install -m 0644 assets/fonts/LICENSE-FontAwesomeFree.txt "$stage/usr/share/licenses/smalltalkai/OFL-FontAwesomeFree.txt"
install -m 0644 web/settings/public/settings-sans-license.txt "$stage/usr/share/licenses/smalltalkai/"
printf 'Installed runtime root; models use the per-user data directory.\n' > "$app/linux-installation"
if [[ "$distro" == fedora ]]; then
  topdir="$work/rpmbuild"
  mkdir -p "$topdir/SOURCES" "$topdir/SPECS" "$work/out"
  cp -a "$stage" "$topdir/SOURCES/stage"
  cp packaging/linux/fedora/smalltalkai.spec "$topdir/SPECS/"
  rpmbuild -bb --define "_topdir $topdir" "$topdir/SPECS/smalltalkai.spec"
  find "$topdir/RPMS" -type f -name '*.rpm' -exec cp -v '{}' "$work/out/" \;
elif [[ "$distro" == arch ]]; then
  pkgdir="$work/arch-package"
  mkdir -p "$pkgdir"
  cp packaging/linux/arch/PKGBUILD "$pkgdir/"
  tar -C "$work" -cf "$pkgdir/smalltalkai-stage.tar" stage
  stage_hash="$(sha256sum "$pkgdir/smalltalkai-stage.tar" | cut -d' ' -f1)"
  sed -i "s/sha256sums=('SKIP')/sha256sums=('${stage_hash}')/" "$pkgdir/PKGBUILD"
  (cd "$pkgdir" && makepkg --noconfirm --cleanbuild --force --nodeps)
  find "$pkgdir" -maxdepth 1 -type f -name '*.pkg.tar.zst' -exec cp -v '{}' "$work/out/" \;
else
  echo "unknown distro: $distro" >&2
  exit 2
fi

cp -a "$work/out/." /out/
printf 'Package contents built for %s\n' "$distro"
