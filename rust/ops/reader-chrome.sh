#!/usr/bin/env bash
set -euo pipefail
args=()
for arg in "$@"; do
    case "$arg" in --no-sandbox|--disable-setuid-sandbox) ;; *) args+=("$arg");; esac
done
exec "$READER_CHROME_EXECUTABLE" --proxy-server=http://127.0.0.1:8302 '--proxy-bypass-list=<-loopback>' --disable-quic --force-webrtc-ip-handling-policy=disable_non_proxied_udp "${args[@]}"
