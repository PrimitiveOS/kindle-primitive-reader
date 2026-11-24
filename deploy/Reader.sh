#!/bin/sh
# Name: Reader
# Author: Primitive AI
# DontUseFBInk
# PrimitiveUSBLease
# PrimitiveViewportHandoff: 2
# Wake: redraw

# Under the launcher SIGUSR1 asks the app to redraw: this script ignores it (a shell ends on it otherwise); the
# app sets its own handler.
[ "${PRIMITIVE_LAUNCHER:-0}" = 1 ] && trap '' USR1
home=${PRIMITIVE_READER_HOME:-/mnt/us/reader}
binary=$home/bin/reader-kindle
[ -x "$binary" ] || exit 1
[ -n "${KL_NATIVE_APP_RUN:-}" ] || exit 1

cp "$binary" "$KL_NATIVE_APP_RUN/run" &&
    chmod 755 "$KL_NATIVE_APP_RUN/run" &&
    cd / || exit 1
exec "$KL_NATIVE_APP_RUN/run" "$@"
