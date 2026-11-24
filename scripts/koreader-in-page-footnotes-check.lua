require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local StyleTweak = require("apps/reader/modules/readerstyletweak")
local Settings = require("luasettings")
local UIManager = require("ui/uimanager")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")

local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height, margin = 720, 480, 32
local fb2 = arg[5] == "fb2"
local classic = arg[5] == "classic"
local tweak_id = fb2 and "footnote-inpage_fb2"
    or classic and "footnote-inpage_classic_classnames" or "footnote-inpage_epub"
local function find_tweak(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == (id or tweak_id) then return item end
        if type(item.sub_item_table) == "table" then
            local child = find_tweak(item.sub_item_table, id)
            if child then return child end
        end
    end
end

G_reader_settings:saveSetting("style_tweaks", {})
local settings = Settings:wrap({})
local applies = 0
local registered = {}
local controller = StyleTweak:new{ ui = {
    menu = { registerToMainMenu = function(_, widget) table.insert(registered, widget) end },
    doc_settings = settings,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applies = applies + 1
    end,
} }
assert(registered[1] == controller)
controller:onReadSettings(settings)
local menu = {}
controller:addToMainMenu(menu)
local row = assert(find_tweak(menu.style_tweaks.sub_item_table))
local master = menu.style_tweaks.sub_item_table[1]
assert(row.enabled_func() and not row.checked_func())
row.callback()
local css = assert(controller:getCssText())
assert(row.checked_func() and applies == 1)
assert(css:find("footnote%-inpage"))
master.callback()
assert(not row.enabled_func() and not controller:getCssText())
master.callback()
assert(row.enabled_func() and row.checked_func())
row.callback()
assert(not row.checked_func() and controller:getCssText() == "")
local details
local show = UIManager.show
UIManager.show = function(_, widget) details = widget end
row.hold_callback({ updateItems = function() end })
UIManager.show = show
assert(details and not details.is_global_default)
details.toggle_global_default_callback()
assert(row.checked_func() and controller.global_tweaks[tweak_id])
row.callback()
assert(not row.checked_func() and controller.doc_tweaks[tweak_id] == false)
controller:onSaveSettings()
controller:onReadSettings(Settings:wrap({}))
assert(row.checked_func() and controller.doc_tweaks[tweak_id] == nil)
controller:onReadSettings(settings)
assert(not row.checked_func() and controller.doc_tweaks[tweak_id] == false)
print("Pinned Style tweaks menu: tap, master switch, hold/default, saved book override")

Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local function open(tweak)
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            assert(flags == 0x03375131)
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(2)
    view:setStyleSheet(fb2 and "./data/fb2.css" or "./data/epub.css", tweak)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontSize(25)
    view:setIntProperty("crengine.footnotes", 1)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(source, false))
    view:renderDocument()
    return view
end

local function position(view, phrase)
    local hits = assert(view:findAllText(phrase, false, false, 8, true, 0, 255))
    assert(#hits == 1, phrase)
    local hit = hits[1]
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    local page = view:getPageFromXPointer(hit.start)
    view:gotoPage(page)
    local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
    print(string.format("page=%d y=%d %s", page, box.y0, phrase))
    return page, box.y0
end

local function capture(view, page, name)
    view:gotoPage(page)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    local separator
    for y = margin, height - margin do
        local gray = frame:getPixel(margin, y):getColor8().a
        if gray < 200 and frame:getPixel(margin + math.floor(width / 7), y):getColor8().a > 235
            and frame:getPixel(margin, y - 1):getColor8().a > 235
            and frame:getPixel(margin, y + 1):getColor8().a > 235 then
            local continuous = true
            for x = margin + 1, margin + math.floor(width / 7) - 1 do
                if frame:getPixel(x, y):getColor8().a ~= gray then
                    continuous = false
                    break
                end
            end
            if continuous then
                assert(not separator)
                separator = y
            end
        end
    end
    print(string.format("capture=%s page=%d separator=%s", name, page, tostring(separator)))
    frame:writePNG(captures .. "/" .. name .. ".png", false)
    frame:free()
    return separator
end

local function width_in_canonical_note(view_, text)
    local hits = assert(view_:findAllText(text, false, false, 8, true, 0, 255))
    assert(#hits == 1, text)
    local hit = hits[1]
    view_:gotoPage(view_:getPageFromXPointer(hit.start))
    local boxes = assert(view_:getWordBoxesFromPositions(hit.start, hit["end"], true))
    assert(#boxes >= 1, text)
    return boxes[#boxes].x1 - boxes[1].x0, boxes[1].x0
end

if classic then
    controller:onReadSettings(Settings:wrap({}))
    assert(row.checked_func())
    local function item(id)
        return assert(find_tweak(menu.style_tweaks.sub_item_table, id))
    end
    -- Match Reader's saved legacy default for these archived style controls.
    local legacy_size = item("inpage_footnote_font-size_smaller")
    legacy_size.callback()
    assert(legacy_size.checked_func())
    local function render_classic(name)
        local candidate = open(assert(controller:getCssText()))
        local reference = position(candidate, "A traditional marker")
        assert(position(candidate, "Classic note sample") > reference)
        local separator = assert(capture(candidate, reference, "stock-classic-note-" .. name))
        local _, indent = width_in_canonical_note(candidate, "Classic note sample")
        local label = width_in_canonical_note(candidate, "raised label")
        local body = width_in_canonical_note(candidate, "A traditional marker")
        candidate:close()
        print(string.format("classic_notes=%s separator=%d x=%d label=%d body=%d", name, separator, indent, label, body))
        return separator, indent, label, body
    end
    local base, x_before, label_before, body_before = render_classic("base")
    local until_entry = item("extend-footnote-inpage_any")
    until_entry.callback()
    assert(until_entry.checked_func())
    local all = render_classic("entry")
    local until_heading = item("extend-footnote-inpage_until_heading")
    until_heading.callback()
    assert(until_heading.checked_func() and not until_entry.checked_func())
    local heading = render_classic("heading")
    assert(all < heading and heading < base)
    for _, id in ipairs({
        "inpage_footnote_text-indent_0", "inpage_footnote_text_justify",
        "inpage_footnote_no_block_spacing", "inpage_footnote_regularize_text",
    }) do
        local option = item(id)
        option.callback()
        assert(option.checked_func())
    end
    local _, x_after, label_after, body_after = render_classic("fixed")
    assert(x_after < x_before and label_after > label_before and body_before == body_after)
    print("Stock classic note aliases, continuation modes and four text fixes verified")
    return
end

if fb2 then
    controller:onReadSettings(Settings:wrap({}))
    assert(row.checked_func())
    local comments = assert(find_tweak(menu.style_tweaks.sub_item_table, "footnote-inpage_fb2_comments"))
    local regular = assert(find_tweak(menu.style_tweaks.sub_item_table, "fb2_footnotes_regular_font_size"))
    local function render_fb2(name, expect_band)
        local candidate = open(assert(controller:getCssText()))
        local reference = position(candidate, "The survey used a limewood ruler")
        assert(position(candidate, "Measured footnote text") > reference)
        assert(position(candidate, "Measured endnote text") > reference)
        assert((capture(candidate, reference, "stock-fb2-" .. name) ~= nil) == expect_band)
        local foot = width_in_canonical_note(candidate, "Measured footnote text")
        local ending = width_in_canonical_note(candidate, "Measured endnote text")
        candidate:close()
        print(string.format("fb2_notes=%s foot=%d end=%d", name, foot, ending))
        return foot, ending
    end
    local notes_width, ordinary_end_width = render_fb2("footnote-only", true)
    row.callback()
    assert(not row.checked_func())
    render_fb2("disabled", false)
    comments.callback()
    assert(comments.checked_func() and not row.checked_func())
    local ordinary_notes_width, end_width = render_fb2("endnote-only", true)
    row.callback()
    assert(row.checked_func() and comments.checked_func())
    render_fb2("both", true)
    regular.callback()
    assert(regular.checked_func() and row.checked_func() and comments.checked_func())
    local regular_notes, regular_end = render_fb2("regular", true)
    assert(notes_width < regular_notes and end_width < regular_end)
    assert(ordinary_notes_width == regular_notes and ordinary_end_width == regular_end)
    return
end

local view = open(css)
local reference = position(view, "The east pier")
local note = position(view, "The repair date")
assert(note > reference)
assert(capture(view, reference, "stock-note-band") == 360)
assert(not capture(view, note, "stock-note-canonical"))
local start = position(view, "The shoal appears")
local body = position(view, "The survey party")
assert(body > start + 2)
assert(capture(view, start, "stock-note-first-slice") == 120)
assert(capture(view, start + 1, "stock-note-only-slice") == 80)
assert(capture(view, body, "stock-note-last-slice") == 400)
local last = position(view, "Soundings end")
assert(last > body)
local previous = body
for i = 1, 17 do
    local page = position(view, string.format("Sounding %02d:", i))
    assert(page >= previous)
    previous = page
end
position(view, "Narrative resumes")
local interleaved_ref = position(view, "The tide gauge showed")
local interleaved_note = position(view, "Tide gauge note")
local interleaved_after = position(view, "The gauge register continues")
assert(interleaved_ref < interleaved_note and interleaved_note <= interleaved_after)
assert(capture(view, interleaved_ref, "stock-note-interleaved-band"))
assert(not capture(view, interleaved_note, "stock-note-interleaved-canonical"))
local illustrated_ref = position(view, "The harbor key explains")
local illustrated_legend = position(view, "The chart legend names")
assert(illustrated_ref <= illustrated_legend)
assert(capture(view, illustrated_ref, "stock-note-illustrated-band"))
if illustrated_legend ~= illustrated_ref then
    capture(view, illustrated_legend, "stock-note-illustrated-page")
end
local forced_ref = position(view, "The keeper marked a second")
local restart = position(view, "The harbor record restarts")
assert(forced_ref < restart)
assert(capture(view, forced_ref, "stock-note-before-hard-break"))
view:close()

local function measure_sizes(name, capture_name)
    local candidate = open(assert(controller:getCssText()))
    local reference_page = position(candidate, "Survey notes for the northern pier")
    local note_page = position(candidate, "Measured note text")
    assert(note_page > reference_page)
    local body = width_in_canonical_note(candidate, "Ordinary body text")
    local note_width = width_in_canonical_note(candidate, "Measured note text")
    local inline_width = width_in_canonical_note(candidate, "inline note label")
    if capture_name then assert(capture(candidate, reference_page, capture_name)) end
    candidate:close()
    print(string.format("note_size=%s body=%d note=%d inline=%d", name, body, note_width, inline_width))
    return {body = body, note = note_width, inline = inline_width}
end

controller:onReadSettings(Settings:wrap({}))
assert(row.checked_func())
local author = measure_sizes("publisher")
local legacy_tweak = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_font-size_smaller"))
legacy_tweak.callback()
assert(legacy_tweak.checked_func())
local legacy = measure_sizes("legacy")
local full_tweak = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_font-size_100"))
full_tweak.callback()
assert(full_tweak.checked_func() and not legacy_tweak.checked_func())
local full = measure_sizes("100", "stock-note-size-100")
local small_tweak = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_font-size_65"))
small_tweak.callback()
assert(small_tweak.checked_func() and not full_tweak.checked_func())
local small = measure_sizes("65", "stock-note-size-65")
assert(author.body == legacy.body and legacy.body == full.body and full.body == small.body)
assert(author.note == legacy.note and author.inline == legacy.inline)
assert(author.note > full.note and author.inline > full.inline)
assert(small.note / full.note > 0.58 and small.note / full.note < 0.71)
assert(small.inline / full.inline > 0.54 and small.inline / full.inline < 0.76)
for _, percent in ipairs({90, 85, 80, 75, 70}) do
    local choice = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_font-size_" .. percent))
    choice.callback()
    assert(choice.checked_func() and not small_tweak.checked_func())
    local measured = measure_sizes(tostring(percent))
    assert(measured.body == full.body)
    assert(math.abs(measured.note / full.note - percent / 100) < 0.055)
end
local ordinary = open("")
local ordinary_reference = position(ordinary, "The east pier")
assert(not capture(ordinary, ordinary_reference, "stock-note-disabled"))
ordinary:close()
print("Pinned KOReader Book mode renders bottom EPUB note bands and continued source lines")
