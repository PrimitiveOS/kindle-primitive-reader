#!/bin/sh
# Name: Reader
# Author: Primitive AI
# DontUseFBInk
# PrimitiveUSBLease

home=${PRIMITIVE_READER_HOME:-/mnt/us/reader}
binary=$home/bin/reader-kindle
[ -x "$binary" ] || exit 1
[ -n "${KL_NATIVE_APP_RUN:-}" ] || exit 1

cp "$binary" "$KL_NATIVE_APP_RUN/run" &&
    chmod 755 "$KL_NATIVE_APP_RUN/run" &&
    cd / || exit 1
exec "$KL_NATIVE_APP_RUN/run" "$@"
