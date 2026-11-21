#!/bin/sh
# Run on the Kindle. Each result is one fresh, offscreen process and scratch profile.
set -eu
umask 077
ulimit -c 0
if [ "$#" -lt 5 ]; then
    echo "Usage: compare-kindle-readers.sh OLD_BINARY NEW_BINARY DOCUMENTS NEW_OUTPUT BOOK [BOOK ...]" >&2
    exit 2
fi
old=$1
new=$2
export PRIMITIVE_READER_DOCUMENTS="$3"
out=$4
shift 4
test -x "$old"
test -x "$new"
test -d "$PRIMITIVE_READER_DOCUMENTS"
mkdir "$out"
export READER_VERIFY_SPINE=all-sections
run_book() {
    variant=$1
    executable=$2
    prefix="$out/$index-$variant"
    if timeout 1200 nice -n 15 "$executable" --check-document "$book" "$prefix-state" >"$prefix.json" 2>"$prefix.stderr"; then
        printf '%s %s passed\n' "$index" "$variant" >>"$out/progress"
    else
        rc=$?
        printf '%s %s failed %s\n' "$index" "$variant" "$rc" >>"$out/progress"
        return "$rc"
    fi
}
index=0
for book do
    index=$((index + 1))
    if [ "$((index % 2))" -eq 1 ]; then
        run_book old "$old"
        run_book new "$new"
    else
        run_book new "$new"
        run_book old "$old"
    fi
done
touch "$out/complete"
