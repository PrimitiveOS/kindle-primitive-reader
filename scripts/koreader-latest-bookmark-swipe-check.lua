-- Execute the pinned ReaderLink and ReaderBookmark callbacks with authored marks.
local function load_methods(path, class, methods)
    local file = assert(io.open(path))
    local source = file:read("*a")
    file:close()
    for _, name in ipairs(methods) do
        local start = assert(source:find("function " .. class .. ":" .. name .. "(", 1, true))
        local finish = assert(source:find("\nend", start, true))
        assert(loadstring(source:sub(start, finish + 3)))()
    end
end
ReaderLink = {}
ReaderBookmark = {}
load_methods(assert(arg[1]), "ReaderLink", { "onSwipe", "onGoToLatestBookmark" })
load_methods(assert(arg[2]), "ReaderBookmark", { "getLatestBookmark" })
BD = { flipDirectionIfMirroredUILayout = function(direction) return direction end }
local follow, latest, found, jumped = false, false, false, nil
isSwipeToFollowNearestLinkEnabled = function() return follow end
isSwipeToJumpToLatestBookmarkEnabled = function() return latest end
isSwipeIgnoreExternalLinksEnabled = function() return true end
local ui = {
    paging = true,
    annotation = { annotations = {
        { datetime = "2026-11-20 12:00:00", page = 12 },
        { datetime = "2026-11-21 12:00:00", page = 4, pos0 = "/body/p[2].4" },
        { datetime = "2026-11-19 12:00:00", page = 15 },
    } },
}
ui.bookmark = setmetatable({ ui = ui }, { __index = ReaderBookmark })
local reader = setmetatable({
    ui = ui,
    onGoToPageLink = function(_, _, internal_only)
        assert(internal_only)
        return found
    end,
    onGotoLink = function(_, link) jumped = link; return true end,
}, { __index = ReaderLink })
local west = { direction = "west" }
assert(not reader:onSwipe(nil, west) and not jumped)
follow, latest, found = true, true, true
assert(reader:onSwipe(nil, west) and not jumped)
found = false
assert(reader:onSwipe(nil, west) and jumped.page == 3)
ui.paging = false
assert(reader:onSwipe(nil, west) and jumped.xpointer == "/body/p[2].4")
assert(jumped.marker_xpointer == jumped.xpointer)
ui.annotation.annotations = {}
jumped = nil
assert(not reader:onSwipe(nil, west) and not jumped)
print("Pinned latest-bookmark swipe respects link priority, PDF page and reflow anchor")
