#!/usr/bin/env python3
"""Produce independent scheduling vectors from an explicit KOReader checkout.

The optional Lua runtime is a local QA dependency. It is never packaged in Reader.
Run with the Python environment containing lupa, then pass the output file to the
ignored upstream_vocabulary_schedule_vectors_match Rust test.
"""
import argparse
import json
from pathlib import Path
import subprocess

from lupa import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--reference", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
source = subprocess.check_output([
    "git", "-C", str(args.reference), "show",
    f"{PIN}:plugins/vocabbuilder.koplugin/db.lua",
], text=True)
start = source.index("function VocabularyBuilder:gotOrForgot(")
end = source.index("\nfunction VocabularyBuilder:batchUpdateItems(", start)
lua = LuaRuntime(unpack_returned_tuples=True)
lua.execute("VocabularyBuilder = {}; qa_time = 0; os.time = function() return qa_time end")
lua.execute(source[start:end])
review = lua.eval("function(item, got, time) qa_time = time; VocabularyBuilder:gotOrForgot(item, got) end")
rows = []
for reviews in range(17):
    for streak in range(80):
        for got in (False, True):
            item = lua.table_from(dict(review_count=reviews, streak_count=streak))
            time = 1_740_777_900 + reviews * 100 + streak
            review(item, got, time)
            rows.append([reviews, streak, got, time, int(item.review_count), int(item.streak_count), int(item.due_time)])
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(rows, separators=(",", ":")) + "\n")
print(f"Generated {len(rows)} scheduling transitions from KOReader {PIN}")
