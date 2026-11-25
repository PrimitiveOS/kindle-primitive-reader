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
Cre.registerFont((font:gsub("Regular.ttf","Bold.ttf")))
Cre.registerFont((font:gsub("Sans%-Regular.ttf","SansArabic-Regular.ttf")))
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
local line = assert(find(menu.style_tweaks.sub_item_table, "no_pseudo_element_first_line"))
local letter = assert(find(menu.style_tweaks.sub_item_table, "no_pseudo_element_first_letter"))
local generated = assert(find(menu.style_tweaks.sub_item_table, "no_pseudo_element_before_after"))
for _, choice in ipairs({line,letter,generated}) do
    assert(choice.enabled_func() and not choice.checked_func())
end
local stable = {}
local words = {"“É”merald", "Harbor", "Ternbird", "Sounding", "Current", "Sails", "Reef", "Shoal", "Channel", "Lagoon", "Lantern", "Beacon", "Shelter", "Bookcase", "صفحة", "كتاب", "دليل", "رحلة", "بحر", "ساحل", "طريق", "مرفأ", "Wavelengths", "Measuring", "Fluxgate", "Surveyor", "Anemone", "Metalwork", "Jurisdiction"}
local function render(state,width)
    local height, margin = 900, 30
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
    local rows = {}
    local function rect(a,b)
        local raw = view:getWordBoxesFromPositions(a,b,true) or {}
        local coords = {}
        for _, piece in ipairs(raw) do
            coords[#coords+1] = ("%d,%d,%d,%d"):format(piece.x0,piece.y0,piece.x1,piece.y1)
        end
        return table.concat(coords, ";")
    end
    local function search(word)
        return view:findAllText(word,true,false,32,true,0,255) or {}
    end
    for _, word in ipairs(words) do
        local hits = search(word)
        assert(#hits == 1, word .. ":" .. #hits)
        local a,b = hits[1].start,hits[1]["end"]
        assert(view:getTextFromXPointers(a,b) == word)
        stable[word] = stable[word] or a .. "|" .. b
        assert(stable[word] == a .. "|" .. b, word .. ": unstable pair")
        view:gotoPage(view:getPageFromXPointer(a))
        local geom = {rect(a,b)}
        assert(#geom[1] > 0, word)
        if word == "“É”merald" or word == "Harbor" or word == "Ternbird" then
            local base,start = a:match("^(.*)%.(%d+)$")
            start = tonumber(start)
            for off = start, start + (word == "“É”merald" and 5 or 1) do
                local left = base .. "." .. off
                local right = base .. "." .. (off+1)
                geom[#geom+1] = rect(left,right)
            end
        end
        rows[word] = table.concat(geom,"|")
        io.write(state,"\t",width,"\t",word,"\t",a,"\t",b,"\t",rows[word],"\n")
    end
    for _, word in ipairs({"Chart","[Chart]"}) do
        local hits = search(word)
        io.write(state,"\t",width,"\t",word,"	HITS	",#hits,"\n")
    end
    local a = search(words[1])[1].start
    local ending = search("Lagoon")[1]["end"]
    local copied = view:getTextFromXPointers(a,ending)
    local expected = "“É”merald Harbor Ternbird Sounding Current Sails Reef Shoal Channel Lagoon"
    assert(copied == expected, copied)
    io.write(state,"\t",width,"	COPY	",copied,"\n")
    view:gotoPage(view:getPageFromXPointer(a))
    local image = BB.new(width,height,BB.TYPE_BB8)
    view:drawCurrentPage(image,false,false,false,false)
    image:writePNG(output .. "/first-line-" .. width .. "-" .. state .. ".png",false)
    image:free()
    view:close()
    return rows
end
for _, width in ipairs({510,650}) do
    local on = render("publisher",width)
    line.callback()
    assert(line.checked_func() and not letter.checked_func() and not generated.checked_func())
    local off = render("line-off",width)
    line.callback()
    letter.callback()
    assert(letter.checked_func() and not line.checked_func())
    render("letter-off",width)
    letter.callback()
    generated.callback()
    assert(generated.checked_func() and not line.checked_func())
    render("prefix-off",width)
    generated.callback()
    local restored = render("restored",width)
    for name, rect in pairs(on) do assert(restored[name] == rect, name) end
    local function bounds(rect)
        local left, top, right, bottom = rect:match("^(-?%d+),(-?%d+),(-?%d+),(-?%d+)")
        left, top, right, bottom = tonumber(left), tonumber(top), tonumber(right), tonumber(bottom)
        return { x=left, y=top, width=right-left, height=bottom-top }
    end
    local function same_row(rows, first, second)
        return bounds(rows[first]).y == bounds(rows[second]).y
    end
    local function glyph_width(word_record, glyph)
        local cells = {}
        for cell in word_record:gmatch("[^|]+") do cells[#cells+1] = cell end
        return bounds(cells[glyph+1]).width
    end
    assert(same_row(off,"“É”merald","Ternbird"))
    assert(not same_row(on,"“É”merald","Ternbird"))
    assert(same_row(on,"“É”merald","Harbor") == (width == 650))
    for glyph = 1,3 do
        assert(glyph_width(on["“É”merald"],glyph) == glyph_width(off["“É”merald"],glyph))
    end
    assert(glyph_width(on["“É”merald"],4) > glyph_width(off["“É”merald"],4))
    assert(bounds(on.Anemone).width == bounds(off.Anemone).width)
    assert(bounds(on.Metalwork).width > 1.4 * bounds(off.Metalwork).width)
    assert(bounds(on.Lantern).width > bounds(off.Lantern).width)
    assert(bounds(on.Shelter).width == bounds(off.Shelter).width)
    assert(bounds(on.Bookcase).width == bounds(off.Bookcase).width)
    if width == 510 then
        assert(not same_row(on,"Wavelengths","Fluxgate") and same_row(off,"Wavelengths","Fluxgate"))
        assert(bounds(on.Wavelengths).height == bounds(off.Wavelengths).height)
    else
        assert(same_row(on,"صفحة","بحر") and not same_row(on,"صفحة","ساحل"))
        assert(same_row(off,"صفحة","ساحل") and same_row(off,"صفحة","طريق"))
        assert(bounds(on["صفحة"]).x > bounds(on["بحر"]).x)
    end
end
assert(applied == 12)
