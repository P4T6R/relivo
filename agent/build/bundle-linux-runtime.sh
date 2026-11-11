#!/usr/bin/env bash
set -euo pipefail
bundle="${1:?Pass the built Linux bundle directory}"
test -x "$bundle/rustdesk"
test -s "$bundle/lib/librustdesk.so"
# These Debian codec ABI versions are absent on the selected Pop!_OS host.
# Bundle their actual libraries and notices; never alias incompatible SONAMEs.
for codec in libvpx.so.7 libyuv.so.0 libjpeg.so.62; do
  cp -L -- "/usr/lib/x86_64-linux-gnu/$codec" "$bundle/lib/$codec"
done
notices="$bundle/share/licenses/native-codecs"
mkdir -p "$notices"
for package in libvpx7 libyuv0 libjpeg62-turbo; do
  cp -- "/usr/share/doc/$package/copyright" "$notices/$package.copyright"
done
dpkg-query -W -f='${Package}\t${Version}\n' libvpx7 libyuv0 libjpeg62-turbo \
  > "$notices/versions.txt"
