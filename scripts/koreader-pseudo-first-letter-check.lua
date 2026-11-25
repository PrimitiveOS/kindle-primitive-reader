require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local config = Settings:wrap({})
local applied = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{ ui = {
    menu = { registerToMainMenu = function() end },
    doc_settings = config,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applied = applied + 1
    end,
} }
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local result = find(item.sub_item_table, id)
            if result then return result end
        end
    end
end
local choice = assert(find(menu.style_tweaks.sub_item_table, "no_pseudo_element_first_letter"))
assert(choice.enabled_func() and not choice.checked_func())
local stable = {}
local function render(state)
    local width, height, margin = 617, 1000, 30
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(26)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local geom = {}
    for _, name in ipairs({"“É”lan", "Breakwater", "Kestrel", "Figural", "Reeds", "Flock", "Marsh", "gathers", "shallows", "Soon", "opens", "trees", "narrow channel", "("}) do
        local hits = view:findAllText(name, true, false, 32, true, 0, 255) or {}
        assert(#hits == 1, name .. ":" .. #hits)
        local a, b = hits[1].start, hits[1]["end"]
        assert(view:getTextFromXPointers(a,b) == name)
        stable[name] = stable[name] or a .. "|" .. b
        assert(stable[name] == a .. "|" .. b)
        local page = view:getPageFromXPointer(a)
        view:gotoPage(page)
        local function rect(a,b)
            local raw = view:getWordBoxesFromPositions(a,b,true) or {}
            local coords = {}
            for _, piece in ipairs(raw) do coords[#coords+1] = ("%d,%d,%d,%d"):format(piece.x0,piece.y0,piece.x1,piece.y1) end
            return table.concat(coords, ";")
        end
        local result = {rect(a,b)}
        local base, offset = a:match("^(.*)%.(%d+)$")
        if base then
            local count = math.min(#name, name == "“É”lan" and 7 or 4)
            offset = tonumber(offset)
            for i=0,count-1 do
                local a = base .. "." .. (offset+i)
                local b = base .. "." .. (offset+i+1)
                result[#result+1] = rect(a,b)
            end
        end
        geom[name] = table.concat(result, "|")
        io.write(state, "\t", name, "\t", a, "\t", geom[name], "\n")
    end
    local first = view:findAllText("“É”lan",true,false,16,true,0,255)[1].start
    view:gotoPage(view:getPageFromXPointer(first))
    local image = BB.new(width,height,BB.TYPE_BB8)
    view:drawCurrentPage(image,false,false,false,false)
    image:writePNG(output .. "/first-letter-" .. state .. ".png",false)
    image:free()
    view:close()
    return geom
end
local on = render("publisher")
choice.callback()
assert(choice.checked_func() and applied == 1)
local off = render("off")
choice.callback()
assert(not choice.checked_func() and applied == 2)
local restored = render("restored")
for word, rect in pairs(on) do assert(rect == restored[word], word) end
local function shape(rect)
    local x,y,r,b = rect:match("^(-?%d+),(-?%d+),(-?%d+),(-?%d+)")
    return (r-x) .. "," .. (b-y)
end
assert(shape(on["Breakwater"]) == shape(off["Breakwater"]), "leading BR")
assert(shape(on["“É”lan"]) ~= shape(off["“É”lan"]), "punctuation/combining initial")
assert(shape(on["Kestrel"]) ~= shape(off["Kestrel"]), "visible inline initial")
assert(shape(on["("]) ~= shape(off["("]), "punctuation-only first text node")
assert(shape(on["Reeds"]) ~= shape(off["Reeds"]), "authored initial after generated prefix")
local function dimensions(rect)
    local x,y,r,b = rect:match("^(-?%d+),(-?%d+),(-?%d+),(-?%d+)")
    return tonumber(x), tonumber(r)-tonumber(x), tonumber(b)-tonumber(y)
end
for _, word in ipairs({"Flock", "Marsh"}) do
    local _, _, shaped = dimensions(on[word])
    local _, _, plain = dimensions(off[word])
    assert(shaped > 2 * plain, word)
end
assert(dimensions(on["narrow channel"]) > dimensions(off["narrow channel"]) + 120)
