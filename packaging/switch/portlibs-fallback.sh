#!/bin/sh
# The toolchain image's fallback when pkg.devkitpro.org refuses the download (it answers some CI
# machines with HTTP 403): devkitPro's base image already has devkitA64, libnx and switch-tools,
# and the game's link needs three portlibs on top (NVK's libvulkan_bh.a: -lexpat -lzstd -lz).
# Each one that is missing is built from its release on GitHub with the Switch's flags.
set -eu
PORTLIBS=/opt/devkitpro/portlibs/switch
CC=/opt/devkitpro/devkitA64/bin/aarch64-none-elf-gcc
AR=/opt/devkitpro/devkitA64/bin/aarch64-none-elf-ar
CFLAGS="-march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIC -ftls-model=local-exec -O2 -ffunction-sections -fdata-sections -D__SWITCH__"
export CC AR CFLAGS
mkdir -p "$PORTLIBS/lib" "$PORTLIBS/include"
work=$(mktemp -d)
cd "$work"

for need in /opt/devkitpro/libnx/lib/libnx.a /opt/devkitpro/tools/bin/elf2nro; do
    [ -e "$need" ] || { echo "error: the base image lacks $need" >&2; exit 1; }
done

if [ ! -f "$PORTLIBS/lib/libz.a" ]; then
    echo "==> zlib from source"
    curl -fsSL https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz | tar xz
    (cd zlib-1.3.1 && CHOST=aarch64-none-elf ./configure --static --prefix="$PORTLIBS" && make -j"$(nproc)" libz.a && make install)
fi

if [ ! -f "$PORTLIBS/lib/libzstd.a" ]; then
    echo "==> zstd from source"
    curl -fsSL https://github.com/facebook/zstd/releases/download/v1.5.6/zstd-1.5.6.tar.gz | tar xz
    (cd zstd-1.5.6/lib && make -j"$(nproc)" libzstd.a ZSTD_MULTITHREAD_SUPPORT=0 \
        && cp libzstd.a "$PORTLIBS/lib/" && cp zstd.h zstd_errors.h zdict.h "$PORTLIBS/include/")
fi

if [ ! -f "$PORTLIBS/lib/libexpat.a" ]; then
    echo "==> expat from source"
    curl -fsSL https://github.com/libexpat/libexpat/releases/download/R_2_6_4/expat-2.6.4.tar.gz | tar xz
    (cd expat-2.6.4 && ./configure --host=aarch64-none-elf --prefix="$PORTLIBS" \
        --disable-shared --enable-static --without-docbook --without-examples --without-tests \
        --without-xmlwf --without-getrandom --without-sys-getrandom \
        CPPFLAGS="-DXML_POOR_ENTROPY" \
        && make -j"$(nproc)" -C lib && make -C lib install)
fi

cd / && rm -rf "$work"
ls -l "$PORTLIBS/lib/libz.a" "$PORTLIBS/lib/libzstd.a" "$PORTLIBS/lib/libexpat.a"
