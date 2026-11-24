#!/usr/bin/env python3
"""Run the pinned KOReader Page Browser row and column callbacks."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SHA256 = "346e5d56fee159920405e57999a47185fe111e759b9cdff23e6eb59855f0d9cd"


def segment(source, begin, end):
    first = source.index(begin)
    return source[first:source.index(end, first + len(begin))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("browser", type=Path, help="pinned frontend/ui/widget/pagebrowserwidget.lua")
    args = parser.parse_args()
    data = args.browser.read_bytes()
    if hashlib.sha256(data).hexdigest() != SHA256:
        parser.error(f"Use the unmodified Page Browser widget from KOReader {PIN}")
    source = data.decode()
    menu = segment(
        source,
        '        {\n            {\n                text = _("Thumbnail columns"),',
        '        {\n            {\n                text = _("Thumbnail page numbers"),',
    )
    defaults = segment(source, "    self.nb_rows = self.ui.doc_settings:readSetting", "    self.nb_grid_items =")
    lua = LuaRuntime()
    lua.execute("""
        PageBrowserWidget = {}
        _ = function(text) return text end
        G_reader_settings = {
            saveSetting = function(self, key, value) self[key] = value end,
            readSetting = function(self, key) return self[key] end,
        }
    """)
    lua.execute("function PageBrowserWidget:controls() return {" + menu + "} end")
    lua.execute("function PageBrowserWidget:loadGrid()\n" + defaults + "\nend")
    lua.execute(segment(source, "function PageBrowserWidget:saveSettings(", "function PageBrowserWidget:updateNbTocSpans("))
    lua.execute(segment(source, "function PageBrowserWidget:updateNbCols(", "function PageBrowserWidget:updateThumbnailPageNumsDisplayType("))
    lua.execute("""
        local function browser()
            return setmetatable({
                min_nb_cols = 1, min_nb_rows = 1, max_nb_cols = 6, max_nb_rows = 6,
                ui = {
                    handmade = {isHandmadeTocEnabled = function() return false end},
                    doc_settings = {
                        saveSetting = G_reader_settings.saveSetting,
                        readSetting = G_reader_settings.readSetting,
                    },
                },
                relayouts = 0,
                updateLayout = function(self) self.relayouts = self.relayouts + 1 end,
            }, {__index = PageBrowserWidget})
        end
        local first = browser()
        first:loadGrid()
        assert(first.nb_cols == 3 and first.nb_rows == 2)
        local buttons = first:controls()
        buttons[1][3].callback()
        buttons[2][3].hold_callback()
        assert(first.nb_cols == 4 and first.nb_rows == 4)
        buttons[1][3].hold_callback()
        assert(first.nb_cols == 6 and not buttons[1][3].enabled_func())
        buttons[2][2].hold_callback()
        assert(first.nb_rows == 2)
        assert(first.relayouts == 4)
        assert(first.ui.doc_settings.page_browser_nb_cols == 6)
        assert(G_reader_settings.page_browser_nb_rows == 2)

        local second = browser()
        second:loadGrid()
        assert(second.nb_cols == 6 and second.nb_rows == 2)
        local second_buttons = second:controls()
        second_buttons[1][2].hold_callback()
        second_buttons[2][2].hold_callback()
        assert(second.nb_cols == 4 and second.nb_rows == 1)
        assert(not second_buttons[2][2].enabled_func())
        first:loadGrid()
        assert(first.nb_cols == 6 and first.nb_rows == 2)
        local third = browser()
        third:loadGrid()
        assert(third.nb_cols == 4 and third.nb_rows == 1)
    """)
    print(f"KOReader {PIN} Lua 5.1: independent grid steps, bounds, per-book saves and defaults passed")


if __name__ == "__main__":
    main()
