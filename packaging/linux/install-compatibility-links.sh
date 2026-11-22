#!/usr/bin/env bash
set -euo pipefail

stage="${1:?expected package staging directory}"
install -d "$stage/usr/lib/simple-stt"
ln -s smalltalkai "$stage/usr/bin/simple-stt"
ln -s smalltalkai-ctl "$stage/usr/bin/simple-stt-ctl"
ln -s ../smalltalkai/smalltalkai-linux "$stage/usr/lib/simple-stt/simple-stt-linux"
ln -s ../smalltalkai/smalltalkai-ctl "$stage/usr/lib/simple-stt/simple-stt-ctl"
ln -s ../smalltalkai/smalltalkai-reader "$stage/usr/lib/simple-stt/readerd"
ln -s smalltalkai-linux.service "$stage/usr/lib/systemd/user/simple-stt-linux.service"
ln -s smalltalkai-server.service "$stage/usr/lib/systemd/user/simple-stt-server.service"
