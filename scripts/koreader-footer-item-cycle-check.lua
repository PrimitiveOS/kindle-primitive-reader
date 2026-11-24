-- Run the pinned ReaderFooter callbacks with authored saved item settings.
-- Usage: lua scripts/koreader-footer-item-cycle-check.lua <readerfooter.lua>
local input = assert(io.open(assert(arg[1])))
local source = input:read("*a")
input:close()
ReaderFooter = {}
for _, name in ipairs({"TapFooter", "onToggleFooterMode"}) do
    local start = assert(source:find("function ReaderFooter:" .. name .. "(", 1, true))
    local finish = assert(source:find("\nend", start, true))
    assert(loadstring(source:sub(start, finish + 3)))()
end

local saved
G_reader_settings = {
    saveSetting = function(_, key, mode)
        assert(key == "reader_footer_mode")
        saved = mode
    end,
}
local footer = setmetatable({
    mode = 1,
    mode_nb = 5,
    mode_list = { off = 0, page_progress = 1, dynamic_filler = 4 },
    mode_index = { "page_progress", "author", "time", "dynamic_filler" },
    settings = { page_progress = true, author = false, time = true, dynamic_filler = true },
    view = { flipping_visible = false },
    applyFooterMode = function() end,
    onUpdateFooter = function() end,
    rescheduleFooterAutoRefreshIfNeeded = function() end,
}, { __index = ReaderFooter })
for _, expected in ipairs({3, 0, 1}) do
    assert(footer:TapFooter())
    assert(footer.mode == expected and saved == expected)
end
footer.settings.lock_tap = true
assert(footer:TapFooter() == nil and footer.mode == 1 and saved == 1)
print("Stock footer cycles enabled items, hides once, skips the filler and respects tap lock")
