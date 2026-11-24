#!/bin/sh
# Run on Kindle. Probe the Oasis 3 hardware gate or stream a fixture's frame.
set -eu
run=${KL_RUN:-/var/tmp/kl}
case "$run" in
    /var/tmp/kl) ;;
    /var/tmp/kl-reader-*)
        name=${run#/var/tmp/kl-reader-}
        case "$name" in ''|*[!a-z0-9-]*) exit 2 ;; esac
        ;;
    *) exit 2 ;;
esac
screen=/sys/class/graphics/fb0
operation=${1:-preflight}
width=1264
height=1680

[ "$(lipc-get-prop com.lab126.powerd state)" = active ] || { echo 'power=inactive'; exit 2; }
[ "$(cat "$screen/bits_per_pixel")" = 8 ] || { echo 'framebuffer=unsupported'; exit 2; }
stride=$(cat "$screen/stride")
[ "$stride" = 1280 ] || { echo 'framebuffer=unsupported'; exit 2; }
turn=$(cat "$screen/rotate")
case "$turn" in 0|2) ;; *) echo 'rotation=unsupported'; exit 2;; esac
geometry=$(cat "$screen/virtual_size")
case "$geometry" in "$stride,"*) ;; *) echo 'framebuffer=unsupported'; exit 2;; esac
rows=${geometry#*,}
pan=$(cat "$screen/pan")
case "$pan" in 0,*) ;; *) echo 'framebuffer=unsupported'; exit 2;; esac
yoffset=${pan#*,}
case "$rows:$yoffset" in :*|*:|*[!0-9:]*) echo 'framebuffer=unsupported'; exit 2;; esac
[ "$((yoffset + height))" -le "$rows" ] || { echo 'framebuffer=unsupported'; exit 2; }
[ -r /dev/fb0 ] || { echo 'framebuffer=unavailable'; exit 2; }
grep -q ' /mnt/base-us ' /proc/mounts || { echo 'storage=unavailable'; exit 2; }

if [ "$operation" = preflight ] && [ "$#" -le 1 ]; then
    touch=
    keys=
    for event in /sys/class/input/event*; do
        name=$(cat "$event/device/name" 2>/dev/null) || continue
        case "$name" in
            cyttsp5_mt) touch=${event##*/} ;;
            gpio-keys) keys=${event##*/} ;;
        esac
    done
    [ -n "$touch" ] && [ -n "$keys" ] || { echo 'input=unsupported'; exit 2; }
    printf 'power=active framebuffer=%sx%s stride=%s rotate=%s pan=%s touch=%s keys=%s\n' "$width" "$height" "$stride" "$turn" "$pan" "$touch" "$keys"
    if [ -e "$run/app.pid" ] || [ -e "$run/next" ]; then
        echo 'foreground=busy'
        exit 2
    fi
    [ -s "$run/launcher.pid" ] || { echo 'foreground=launcher-unavailable'; exit 2; }
    echo 'foreground=available'
    exit 0
fi

check_overlay() {
    viewport=$(tr '\000' '\n' < "/proc/$pid/environ" | sed -n 's/^KL_APP_VIEWPORT_FILE=//p')
    case "$viewport" in "$run"/app-turn.*/viewport) ;; *) return 1 ;; esac
    directory=${viewport%/viewport}
    scope=${directory#"$run"/}
    case "$scope" in */*) return 1 ;; esac
    start=$(awk '{ print $22 }' "/proc/$pid/stat") || return 1
    offer=$(cat "$run/app-viewport-offer") || return 1
    [ "$offer" = "v1 $pid $start $scope" ] || return 1
    IFS=' ' read -r version phase owner ticks token generation logical logical_w logical_h \
        panel_w panel_h rotation offset pitch extra < "$viewport" || return 1
    case "$logical" in
        U|D) expected="$width $height" ;;
        CW|CCW) expected="$height $width" ;;
        *) return 1 ;;
    esac
    [ "$version $phase $owner $ticks $token" = "v2 ready $pid $start $scope" ] &&
        [ "$logical_w $logical_h" = "$expected" ] &&
        [ "$panel_w $panel_h $rotation $offset $pitch" = "$width $height $turn $yoffset $stride" ] &&
        [ -z "$extra" ] || return 1
    case "$generation" in ''|*[!0-9]*) return 1 ;; esac
    [ "$(cat "$directory/launcher-page-key-cutoff")" = held ] &&
        [ "$(cat "$directory/launcher-occluded")" = "$((generation + 1))" ]
}

if { [ "$operation" = capture ] || [ "$operation" = capture-overlay ]; } && [ "$#" -eq 2 ]; then
    [ -d "$2" ] || exit 2
    scratch=$(cd "$2" && pwd -P)
    case "$scratch" in /mnt/us/reader/tests/*/foreground) ;; *) exit 2;; esac
    pid=$(cat "$run/app.pid")
    case "$pid" in ''|*[!0-9]*) exit 2;; esac
    check_fixture() {
        [ "$(cat "$run/app.pid")" = "$pid" ] &&
            [ "$(cat "$screen/rotate")" = "$turn" ] &&
            [ "$(cat "$screen/pan")" = "$pan" ] &&
            [ "$(readlink "/proc/$pid/exe")" = "$run/native-app/run" ] &&
            tr '\000' '\n' < "/proc/$pid/environ" | grep -Fqx "PRIMITIVE_READER_HOME=$scratch" || return 1
        state=$(awk '/^State:/ { print $2 }' "/proc/$pid/status") || return 1
        case "$operation:$state" in
            capture:T|capture:t) return 1 ;;
            capture:?) return 0 ;;
            capture-overlay:T|capture-overlay:t) check_overlay ;;
            *) return 1 ;;
        esac
    }
    check_fixture || exit 2
    frame=$(mktemp "$scratch/foreground-frame.XXXXXX")
    trap 'rm -f "$frame"' EXIT
    dd if=/dev/fb0 of="$frame" bs="$stride" skip="$yoffset" count="$height" 2>/dev/null
    check_fixture || exit 2
    [ "$(wc -c < "$frame")" -eq "$((stride * height))" ] || exit 2
    cat "$frame"
    exit 0
fi
printf 'Usage: sh verify-oasis-foreground.sh [preflight | capture <fixture-home> | capture-overlay <fixture-home>]\n' >&2
exit 1
