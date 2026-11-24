#!/usr/bin/env python3
"""Run pinned KOReader dictionary close callbacks with a small UI test double."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SHA256 = "1097fe3fbf4ef26c4c1b259d044af72f8258523537d73ad843ec5ce3a7c281bd"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path, help="pinned frontend/ui/widget/dictquicklookup.lua")
args = parser.parse_args()
raw = args.source.read_bytes()
if hashlib.sha256(raw).hexdigest() != SHA256:
    parser.error(f"Use the unmodified dictquicklookup.lua from KOReader {PIN}")
source = raw.decode("utf-8")
start = source.index("function DictQuickLookup:onClose(no_clear)")
end = source.index("\nfunction DictQuickLookup:onSwipe", start)

lua = LuaRuntime(unpack_returned_tuples=True)
lua.execute("DictQuickLookup = {}")
lua.execute(source[start:end])
close = lua.eval("""function(depth, hold, armed)
    local trace = {}
    local function record(value) table.insert(trace, value) end
    local windows = {}
    DictQuickLookup.window_list = windows
    UIManager = {
        close = function(_, target)
            record(target.name)
            for index, window in ipairs(windows) do
                if window == target then table.remove(windows, index); break end
            end
        end,
        scheduleIn = function(_, _, callback) callback() end,
    }
    G_defaults = {readSetting = function() return 0 end}
    for index = 1, depth do
        local window = {
            name = "close:" .. index,
            menu_opened = {},
            dict_close_callback = function() record("return:" .. index) end,
        }
        if index == 1 then
            window.save_highlight = armed
            window.highlight = {
                getClearId = function() return 1 end,
                clear = function() record("clear:1") end,
                saveHighlight = function() record("save:1") end,
            }
        end
        windows[index] = setmetatable(window, {__index = DictQuickLookup})
    end
    local results = {name = "close:results"}
    windows[depth].menu_opened[results] = true
    if hold then windows[depth]:onHoldClose() else windows[depth]:onClose() end
    return table.concat(trace, ","), #windows
end""")

assert close(3, False, True) == ("close:results,close:3,return:3", 2)
assert close(3, True, True) == (
    "close:results,close:3,return:3,close:2,return:2,close:1,save:1,clear:1,return:1",
    0,
)
assert close(1, True, False) == ("close:results,close:1,clear:1,return:1", 0)
print(f"KOReader {PIN} on {lua.lua_implementation}: tap closes one, hold closes the lookup stack and saves the armed highlight once")
