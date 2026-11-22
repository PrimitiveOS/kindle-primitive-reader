"""Produce authored custom-flow expectations with the pinned KOReader Lua methods.

Run with a Python environment providing lupa, the KOReader source directory and
an output JSON path. No document, device state or rendering engine is opened.
"""
import hashlib
import json
from pathlib import Path
import random
import sys

from lupa import LuaRuntime


def install_method(runtime, source, class_name, name):
    start = source.index(f"function {class_name}:{name}(")
    finish = source.find("\nfunction ", start + 1)
    if finish < 0:
        finish = source.index(f"\nreturn {class_name}", start)
    runtime.execute(source[start:finish])


root, output = map(Path, sys.argv[1:])
handmade_path = root / "frontend/apps/reader/modules/readerhandmade.lua"
cre_path = root / "frontend/document/credocument.lua"
handmade, cre = handmade_path.read_text(), cre_path.read_text()
runtime = LuaRuntime(unpack_returned_tuples=True)
runtime.execute("""
ReaderHandMade = {}
CreDocument = {}
package.preload['document/credocument'] = function() return CreDocument end
function make_reader(count)
    local document = {getPageCount = function() return count end}
    local reader = setmetatable({
        flows_enabled = true, flow_points = {}, document = document,
        ui = {annotation = {setNeedsUpdateFlag = function() end}},
    }, {__index = ReaderHandMade})
    reader:setupFlows(true)
    return reader, document
end
""")
for name in ["getNextPage", "getPrevPage"]:
    install_method(runtime, cre, "CreDocument", name)
for name in ["_getItemIndex", "isInHiddenFlow", "toggleHiddenFlow", "updateDocFlows", "setupFlows"]:
    install_method(runtime, handmade, "ReaderHandMade", name)

sequences = [(12, pages) for pages in [[], [2, 5], [0, 4], [7], [2, 5, 3], [2, 5, 3, 3], [2, 4, 7, 10, 1], [0]]]
randomizer = random.Random(20261122)
for _ in range(64):
    sequences.append((37, [randomizer.randrange(37) for _ in range(randomizer.randrange(1, 28))]))
cases = []
for count, toggles in sequences:
    reader, document = runtime.globals().make_reader(count)
    for page in toggles:
        reader.toggleHiddenFlow(reader, page + 1)
    points = [{"page": point.page - 1, "hidden": point.hidden}
              for _, point in sorted(reader.flow_points.items())]
    flows = [document.getPageFlow(document, page + 1) for page in range(count)]
    next_pages = [document.getNextPage(document, page + 1) - 1 for page in range(count)]
    last_regular = max((page for page, flow in enumerate(flows) if flow == 0), default=-1)
    reported_last = document.getLastLinearPage(document) - 1
    defects = {}
    for page, actual in enumerate(next_pages):
        logical = next((candidate for candidate in range(page + 1, count)
                        if flows[candidate] == 0 or flows[candidate] == flows[page]), -1)
        if logical != actual:
            # readerhandmade's final-flow bound compares a half-open end to the
            # inclusive page count. A regular final page can become unreachable.
            assert actual == -1 and logical == last_regular == count - 1
            assert reported_last < last_regular and flows[logical] == 0
            defects[str(page)] = logical
    cases.append({
        "count": count, "toggles": toggles, "points": points,
        "inactive": [index - 1 for _, index in sorted(reader.inactive_flow_points.items())],
        "regular_pages": document.getTotalPagesInFlow(document, 0),
        "next": next_pages,
        "next_reference_defects": defects,
        "reported_last_regular": reported_last,
        "last_regular": last_regular,
        "previous": [document.getPrevPage(document, page + 1) - 1 for page in range(count)],
        "hidden": [document.getPageFlow(document, page + 1) != 0 for page in range(count)],
    })
result = {
    "source": {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
               for path in [handmade_path, cre_path]},
    "cases": cases,
}
output.write_text(json.dumps(result, indent=2) + "\n")
print(f"Compared {len(cases)} authored marker sequences with pinned reference methods.")
print(f"Recorded {sum(len(case['next_reference_defects']) for case in cases)} final-page reference defects.")
