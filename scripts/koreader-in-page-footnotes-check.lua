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
local narrow_ruby = arg[5] == "inline-ruby-narrow"
local body_table = arg[5] == "body-table"
local rtl_note = arg[5] == "rtl-note"
local width, height, margin = narrow_ruby and 310 or 720, body_table and 520 or 480, 32
local fb3_note = arg[5] == "fb3"
local fb2 = arg[5] == "fb2" or fb3_note
local classic = arg[5] == "classic"
local wikipedia = arg[5] == "wikipedia" or arg[5] == "inline-root-wiki"
local tweak_id = fb2 and "footnote-inpage_fb2"
    or classic and "footnote-inpage_classic_classnames"
    or wikipedia and "footnote-inpage_wikipedia" or "footnote-inpage_epub"
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
if rtl_note then Cre.registerFont(assert(arg[6])) end

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
    if rtl_note then
        view:setStringProperty("crengine.font.fallback.faces", "Noto Sans Arabic")
    end
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

local function separator_row(frame, right)
    local separator
    local line_width = math.floor(width / 7)
    local left = right and width - margin - line_width or margin
    for y = margin, height - margin do
        local gray = frame:getPixel(left, y):getColor8().a
        if gray < 200 and frame:getPixel(left + line_width, y):getColor8().a > 235
            and frame:getPixel(left, y - 1):getColor8().a > 235
            and frame:getPixel(left, y + 1):getColor8().a > 235 then
            local continuous = true
            for x = left + 1, left + line_width - 1 do
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
    return separator
end

local function draw_page(view, page)
    view:gotoPage(page)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    return frame
end

local function capture(view, page, name, right)
    local frame = draw_page(view, page)
    local separator = separator_row(frame, right)
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

if rtl_note then
    controller:onReadSettings(Settings:wrap({}))
    local function geometry(view, phrase)
        local page, y = position(view, phrase)
        local hit = view:findAllText(phrase, false, false, 8, true, 0, 255)[1]
        local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        assert(box.y0 == y)
        return page, box, hit.start
    end
    local function raster(frame, y, rows)
        local pixels = {}
        for row = y, y + rows - 1 do
            for x = margin, width - margin - 1 do
                pixels[#pixels + 1] = string.char(frame:getPixel(x, row):getColor8().a)
            end
        end
        return table.concat(pixels)
    end
    local function measure(phase, enabled)
        local view = open(assert(controller:getCssText()))
        local page, body, dominant = geometry(view, "يسجل التقرير")
        local original, first, head = geometry(view, "أظهر")
        local middle_page, middle, latin = geometry(view, "S4")
        local end_page, last, finish = geometry(view, "الغروب")
        local return_page, backlink, return_pointer = geometry(view, "العودة إلى التقرير")
        assert(original > page and original == middle_page and middle_page == end_page and original == return_page)
        assert(first.x0 > middle.x0 and middle.x0 > last.x0 and first.y0 == last.y0)
        local x, y = math.floor((backlink.x0 + backlink.x1) / 2), math.floor((backlink.y0 + backlink.y1) / 2)
        assert(view:getLinkFromPosition(x, y):match(" rtl%-local%-ref$"))
        local body_frame = draw_page(view, page)
        assert(not separator_row(body_frame, false))
        assert((separator_row(body_frame, true) ~= nil) == enabled)
        local marker
        for _, link in ipairs(view:getPageLinks(true)) do
            if link.section:match(" rtl%-local%-note$") then
                local b = link.segments[1]
                marker = view:getLinkFromPosition(math.floor((b.x0 + b.x1)/2), math.floor((b.y0 + b.y1)/2))
            end
        end
        assert(marker and marker:match(" rtl%-local%-note$"))
        local separator = capture(view, page, "stock-rtl-note-" .. phase, true)
        if enabled then
            assert(not capture(view, original, "stock-rtl-note-original", true))
            local note = draw_page(view, original)
            local rows = backlink.y1 - first.y0
            local sample = raster(note, first.y0, rows)
            local offset
            for at = separator + 1, height - margin - rows do
                if raster(body_frame, at, rows) == sample then offset = at - first.y0; break end
            end
            assert(offset, "copied RTL note must preserve both original lines")
            note:free()
            view:gotoPage(page)
            assert(view:getLinkFromPosition(x, y + offset) == "")
        end
        body_frame:free()
        view:close()
        return table.concat({body.x0, body.y0, dominant, head, latin, finish, return_pointer}, "|")
    end
    row.callback()
    assert(not row.checked_func())
    local ordinary = measure("off", false)
    row.callback()
    assert(row.checked_func())
    assert(measure("band", true) == ordinary)
    local tie = open(assert(controller:getCssText()))
    local first_page = geometry(tie, "A second bilingual note")
    local note_page, arabic = geometry(tie, "الحاشية تفاصيل")
    local english_page, english = geometry(tie, "The paired line")
    assert(note_page > first_page and note_page == english_page)
    assert(arabic.y0 < english.y0 and arabic.x0 > english.x0)
    local tie_frame = draw_page(tie, first_page)
    assert(not separator_row(tie_frame, true))
    tie_frame:free()
    assert(capture(tie, first_page, "stock-rtl-note-tie", false))
    tie:close()
    row.callback()
    assert(not row.checked_func())
    assert(measure("restored", false) == ordinary)
    print("Pinned RTL note copies preserve Arabic order and put the separator on the right by majority")
    return
end

if arg[5] == "inline-root-epub" or arg[5] == "inline-root-wiki" then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_all_inline"))
    local prefix = "stock-note-" .. arg[5] .. "-"
    local body = wikipedia and "The catalog lists the register" or "The guide lists the latch"
    local intro = wikipedia and "Register note." or "Latch record."
    local tail = wikipedia and "The date is missing." or "The brass pin stayed."
    local target = wikipedia and "cite_ref_register" or "root-epub-ref"
    local marker = wikipedia and "IX." or "7."
    local function measure(phase, shared_line)
        local view = open(assert(controller:getCssText()))
        local reference, body_y = position(view, body)
        assert(capture(view, reference, prefix .. phase))
        local original, intro_y = position(view, intro)
        local text, tail_y = position(view, tail)
        assert(text == original and original > reference)
        assert(shared_line and intro_y == tail_y or not shared_line and intro_y < tail_y)
        local _, intro_x = width_in_canonical_note(view, intro)
        local _, tail_x = width_in_canonical_note(view, tail)
        local frame = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(frame, false, false, false, false)
        local marker_x
        for x = margin, intro_x - 1 do
            for y = intro_y, intro_y + 28 do
                if frame:getPixel(x, y):getColor8().a < 180 then
                    marker_x = x
                    break
                end
            end
            if marker_x then break end
        end
        frame:free()
        assert(marker_x and not view:findAllText(marker .. " " .. intro, false, false, 8, true, 0, 255))
        local backlink = false
        for _, link in ipairs(view:getPageLinks(true)) do
            if link.section:sub(-#target) == target then
                for _, segment in ipairs(link.segments) do
                    local x = math.floor((segment.x0 + segment.x1) / 2)
                    local y = math.floor((segment.y0 + segment.y1) / 2)
                    backlink = backlink or view:getLinkFromPosition(x, y) == link.section
                end
            end
        end
        assert(backlink)
        print(("Inside marker %s x=%d, text x=%d/%d, y=%d/%d"):format(
            marker, marker_x, intro_x, tail_x, intro_y, tail_y))
        if shared_line then
            assert(tail_x > intro_x)
            assert(not capture(view, original, prefix .. "original"))
        end
        view:close()
        return body_y, marker_x, intro_x
    end
    assert(not option.checked_func())
    local plain_y, plain_marker, plain_text = measure("before", false)
    option.callback()
    assert(option.checked_func())
    local body_y, inside_marker, inside_text = measure("after", true)
    assert(body_y == plain_y and inside_marker > plain_marker and inside_text > plain_text)
    print("Pinned note-root marker moves inside and retains its canonical backlink")
    return
end

if arg[5] == "inline-image" or arg[5] == "inline-block-image" then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_all_inline"))
    local ffi = require("ffi")
    ffi.cdef[[void free(void *);]]
    local block = arg[5] == "inline-block-image"
    local prefix = "stock-note-" .. arg[5] .. "-"
    local body_text = block and "The hinge measured" or "The carpenter read the pencilled instructions"
    local before_image = block and "Measurements in the workshop." or "Cabinet diagram legend."
    local after_image = block and "A hinge holds the door." or "for shelf depth."
    local target = block and "isolated-start" or "media-key"
    local function draw(view, page)
        view:gotoPage(page)
        local frame = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(frame, false, false, false, false)
        return frame
    end
    local function raster(frame, x0, y0, image_width, image_height)
        local data = {}
        for y = y0, y0 + image_height - 1 do
            for x = x0, x0 + image_width - 1 do
                data[#data + 1] = string.char(frame:getPixel(x, y):getColor8().a)
            end
        end
        return table.concat(data)
    end
    local function measure(phase, same_line)
        local image_width, image_height = 56, 36
        if block then
            image_width, image_height = 216, 138
            if same_line then image_width, image_height = 108, 69 end
        end
        local view = open(assert(controller:getCssText()))
        local reference, body_y = position(view, body_text)
        local separator = assert(capture(view, reference, prefix .. phase))
        local original, intro_y = position(view, before_image)
        local text, caption_y = position(view, after_image)
        assert(text == original and original > reference)
        assert(same_line and intro_y == caption_y or not same_line and intro_y < caption_y)

        local frame = draw(view, original)
        local box, png
        for _, link in ipairs(view:getPageLinks(true)) do
            if link.section:sub(-#target) == target then
                assert(#link.segments == 1)
                box = link.segments[1]
                assert(box.x1 - box.x0 == image_width)
                local x = math.floor((box.x0 + box.x1) / 2)
                local y = math.floor((box.y0 + box.y1) / 2)
                assert(view:getLinkFromPosition(x, y) == link.section)
                local data, bytes = view:getImageDataFromPosition(x, y)
                assert(data and bytes > 24)
                png = ffi.string(data, bytes)
                ffi.C.free(data)
                assert(png:sub(1, 8) == "\137PNG\r\n\26\n")
            end
        end
        assert(box and png)
        local image = raster(frame, box.x0, box.y0, image_width, image_height)
        assert(image:find(string.char(35), 1, true))
        frame:free()
        local copy = draw(view, reference)
        local copy_y
        for y = separator + 1, height - margin - image_height do
            if raster(copy, box.x0, y, image_width, image_height) == image then
                copy_y = y
                break
            end
        end
        copy:free()
        assert(copy_y, "the reference band must preserve the complete image")
        if block then
            local x = box.x0 + image_width / 2
            local y = copy_y + math.floor(image_height / 2)
            assert(not view:getImageDataFromPosition(x, y))
            assert(view:getLinkFromPosition(x, y) == "")
            print("The stock strip paints the image without a tap target")
        end
        print(("Image %d×%d x=%d, note y=%d, copy y=%d, PNG=%d bytes"):format(
            image_width, image_height, box.x0, box.y0, copy_y, #png))
        if same_line then
            assert(not capture(view, original, prefix .. "original"))
        end
        view:close()
        return body_y, png, image
    end
    local old_body, old_png, old_pixels = measure("before", false)
    assert(not option.checked_func())
    option.callback()
    assert(option.checked_func())
    local body, png, pixels = measure("after", true)
    assert(body == old_body and png == old_png and (block or pixels == old_pixels))
    print("Pinned whole-note inline toggle reflows around unchanged linked artwork")
    return
end

if arg[5] == "inline-list" or arg[5] == "inline-nested" then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table,
        "inpage_footnote_all_inline"))
    local nested = arg[5] == "inline-nested"
    local name = nested and "nested" or "list"
    local body = nested and "The entry lists the compartments" or "The final ledger"
    local phrases = nested and { "Drawer notes.", "Upper shelf.", "Oak lip.", "Tin key", "Low bin." }
        or { "Inventory number eight.", "North lantern.", "Spare wick" }
    local function measure(phase, same_line)
        local view = open(assert(controller:getCssText()))
        local reference, body_y = position(view, body)
        local prefix = "stock-note-inline-" .. name .. "-"
        assert(capture(view, reference, prefix .. phase))
        local original, previous_y, previous_x
        for _, phrase in ipairs(phrases) do
            local page, y = position(view, phrase)
            original = original or page
            assert(original == page and page > reference)
            local _, x = width_in_canonical_note(view, phrase)
            if previous_y then
                if same_line then
                    assert(previous_y == y and previous_x < x)
                else
                    assert(previous_y < y)
                end
            end
            previous_y, previous_x = y, x
        end
        if same_line then
            assert(not capture(view, original, prefix .. "original"))
            if nested then
                local target = "nested-ref"
                local backlink
                for _, link in ipairs(view:getPageLinks(true)) do
                    if link.section:sub(-#target) == target then
                        backlink = link
                        break
                    end
                end
                assert(backlink)
                local hit = view:getLinkFromPosition(backlink.start_x + 3, backlink.start_y + 4)
                assert(hit == backlink.section)
            end
        end
        view:close()
        return body_y
    end
    assert(not option.checked_func())
    local before = measure("before", false)
    option.callback()
    assert(option.checked_func())
    assert(measure("after", true) == before)
    print("Pinned whole-note inline toggle flattens list items on the original note and its page copy")
    return
end

if arg[5] == "inline-all" then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table,
        "inpage_footnote_all_inline"))
    local function measure(name, same_line)
        local view = open(assert(controller:getCssText()))
        local page, body_y = position(view, "The harbor chart")
        assert(capture(view, page, "stock-note-inline-" .. name))
        local number, number_y = position(view, "[7]")
        local text, text_y = position(view, "signal lamp")
        assert(number == text and text > page)
        assert((number_y == text_y) == same_line)
        local _, number_x = width_in_canonical_note(view, "[7]")
        local _, text_x = width_in_canonical_note(view, "signal lamp")
        if same_line then
            assert(text_x > number_x)
            capture(view, text, "stock-note-inline-original")
        end
        view:close()
        return body_y
    end
    assert(not option.checked_func())
    local before = measure("before", false)
    option.callback()
    assert(option.checked_func())
    assert(measure("after", true) == before)
    option.callback()
    assert(not option.checked_func())
    print("Pinned whole-note inline toggle shares the canonical line and keeps ordinary body unchanged")
    return
end

if arg[5] == "inline-generated" then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table,
        "inpage_footnote_all_inline"))
    local pointers = {}
    local function inspect(view, phrase)
        local page, y = position(view, phrase)
        local hit = view:findAllText(phrase, false, false, 8, true, 0, 255)[1]
        assert(pointers[phrase] == nil or pointers[phrase] == hit.start, phrase)
        pointers[phrase] = hit.start
        local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        local node = hit.start:match("^(.-/q)/ruby/") or hit.start
        local markup = assert(view:getHTMLFromXPointer(node, 0x0C40, true))
        local tags = {}
        for tag in markup:gmatch("<pseudoElem ([^>]+)") do
            tags[#tags + 1] = tag
        end
        print("note-generated", phrase, table.concat(tags, " | "))
        return {page = page, y = y, box = box, tags = table.concat(tags, " | ")}
    end
    local function measure(phase, inline)
        local view = open(assert(controller:getCssText()))
        local body = inspect(view, "The harbor packet")
        local memo = inspect(view, "memo")
        local seal = inspect(view, "river seal")
        local buoy = inspect(view, "blue buoy")
        local tail = inspect(view, "Keep a duplicate")
        local following = inspect(view, "Following remark")
        assert(body.page < memo.page and memo.page == seal.page and seal.page == buoy.page
            and buoy.page == tail.page and tail.page == following.page)
        assert(memo.tags:find("[", 1, true) and memo.tags:find("]", 1, true))
        assert(seal.tags:find("‘", 1, true) and seal.tags:find("’", 1, true))
        assert(buoy.tags:find("‘", 1, true) and buoy.tags:find("’", 1, true))
        assert(following.tags:find("‘", 1, true) and following.tags:find("’", 1, true))
        for _, token in ipairs({"[memo]", "‘", "’"}) do
            local hits = view:findAllText(token, false, false, 8, true, 0, 255) or {}
            for _, hit in ipairs(hits) do
                -- CRE folds curly quotes onto earlier authored ASCII apostrophes.
                assert(not hit.start:find("/DocFragment[29]/", 1, true), token)
                assert(view:getTextFromXPointers(hit.start, hit["end"]) ~= token, token)
            end
        end
        view:gotoPage(seal.page)
        assert((view:getLinkFromPosition(seal.box.x0 + 5, math.floor((seal.box.y0 + seal.box.y1) / 2))
            or ""):find("generated%-ref"))
        if inline then
            assert(memo.y == seal.y and seal.y == tail.y)
            assert(memo.box.x0 < seal.box.x0 and seal.box.x0 < buoy.box.x0 and buoy.box.x0 < tail.box.x0)
            assert(not capture(view, memo.page, "stock-note-inline-generated-original"))
        else
            assert(memo.y < seal.y and seal.y < tail.y)
        end
        assert(capture(view, body.page, "stock-note-inline-generated-" .. phase))
        view:close()
        return body.y
    end
    assert(not option.checked_func())
    local y = measure("before", false)
    option.callback()
    assert(option.checked_func() and measure("after", true) == y)
    option.callback()
    assert(not option.checked_func() and measure("restored", false) == y)
    print("Pinned joined note decorations: canonical word pointers, link, source-free marks, and restored baseline")
    return
end

if arg[5] == "inline-ruby" or narrow_ruby then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table,
        "inpage_footnote_all_inline"))
    local prefix = "stock-note-" .. arg[5] .. "-"
    local function placed(view, phrase)
        local page, y = position(view, phrase)
        local hit = view:findAllText(phrase, false, false, 8, true, 0, 255)[1]
        local boxes = view:getWordBoxesFromPositions(hit.start, hit["end"], true)
        local box = boxes[1]
        assert(box.y0 == y)
        return {page = page, box = box, one_line = #boxes == 1, pointer = hit.start}
    end
    local function frame(view, page)
        view:gotoPage(page)
        local buffer = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(buffer, false, false, false, false)
        return buffer
    end
    local function raster(buffer, box, y)
        local data = {}
        for row = y, y + box.y1 - box.y0 - 1 do
            for x = box.x0, box.x1 - 1 do
                data[#data + 1] = string.char(buffer:getPixel(x, row):getColor8().a)
            end
        end
        return table.concat(data)
    end
    local function measure(phase, inline)
        local view = open(assert(controller:getCssText()))
        local body = placed(view, "The ferry reports")
        local head = placed(view, "Ledger head")
        local first = placed(view, "bright water")
        local second = placed(view, "quiet letter")
        local first_rt = placed(view, "reading gloss")
        local second_rt = placed(view, "silent gloss")
        local tail = placed(view, "Ledger tail")
        assert(body.page < head.page)
        for _, item in ipairs({first, second, first_rt, second_rt, tail}) do
            assert(item.page == head.page)
        end
        assert(first_rt.box.y0 < first.box.y0 and second_rt.box.y0 < second.box.y0)
        assert(first.one_line and second.one_line)
        if inline then
            if narrow_ruby then
                assert(first.box.y0 < second.box.y0)
                assert(tail.box.y0 == second_rt.box.y0 and tail.box.x0 > second.box.x0)
            else
                assert(first.box.y0 == second.box.y0 and first.box.x0 < second.box.x0)
                assert(head.box.y0 == first_rt.box.y0 and first_rt.box.y0 == second_rt.box.y0)
                assert(tail.box.y0 == head.box.y0 and tail.box.x0 > second.box.x0)
            end
            assert(not capture(view, head.page, prefix .. "original"))
        else
            assert(head.box.y0 < first.box.y0 and first.box.y0 < second.box.y0)
            assert(second.box.y0 < tail.box.y0)
        end
        local separator = assert(capture(view, body.page, prefix .. phase))
        local original, copy = frame(view, head.page), frame(view, body.page)
        local offset
        for _, item in ipairs({first_rt, second_rt}) do
            local box = item.box
            local ink = raster(original, box, box.y0)
            assert(ink:find("[%z\1-\128]"), "annotation must paint dark pixels")
            if not offset then
                for y = separator + 1, height - margin - (box.y1 - box.y0) do
                    if raster(copy, box, y) == ink then
                        offset = y - box.y0
                        break
                    end
                end
            end
            assert(offset and raster(copy, box, box.y0 + offset) == ink)
        end
        original:free()
        copy:free()
        local x = first.box.x0 + 5
        local y = math.floor((first.box.y0 + first.box.y1) / 2)
        assert(view:getLinkFromPosition(x, y + offset) == "")
        local copy_hit = view:getTextFromPositions(x, y + offset, x, y + offset, false, false)
        assert(not copy_hit or not copy_hit.text:find("bright", 1, true))
        view:gotoPage(head.page)
        assert(view:getLinkFromPosition(x, y):match(" ruby%-ref$"))
        local canonical_hit = assert(view:getTextFromPositions(x, y, x, y, false, false))
        assert(canonical_hit.text:match("^bright"))
        view:close()
        return table.concat({body.box.y0, first.pointer, second.pointer, first_rt.pointer,
            second_rt.pointer}, "|")
    end
    assert(not option.checked_func())
    local before = measure("before", false)
    option.callback()
    assert(option.checked_func())
    assert(measure("after", true) == before)
    option.callback()
    assert(not option.checked_func())
    assert(measure("restored", false) == before)
    print("Pinned EPUB note keeps both ruby annotations and their canonical text positions")
    return
end

if arg[5] == "inline-table" or body_table then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table, "inpage_footnote_all_inline"))
    local function geometry(view, phrase)
        local page, y = position(view, phrase)
        local hit = view:findAllText(phrase, false, false, 8, true, 0, 255)[1]
        local box = view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1]
        assert(box.y0 == y)
        return page, box, hit.start
    end
    local function draw(view, page)
        view:gotoPage(page)
        local frame = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(frame, false, false, false, false)
        return frame
    end
    local function outlined_rows(frame, first, last)
        local x0, x1, y0, y1
        for y = first, last do
            local start
            for x = margin, width - margin do
                local dark = x < width - margin and frame:getPixel(x, y):getColor8().a < 32
                if dark and not start then start = x end
                if not dark and start then
                    if x - start >= 150 then
                        if not y0 then x0, x1, y0 = start, x, y end
                        if start == x0 and x == x1 then y1 = y end
                    end
                    start = nil
                end
            end
        end
        assert(y0 and y1 and y1 - y0 > 20)
        return x0, x1, y0, y1 + 1
    end
    local function raster(frame, x0, x1, y0, y1)
        local data = {}
        for y = y0, y1 - 1 do
            for x = x0, x1 - 1 do data[#data + 1] = string.char(frame:getPixel(x, y):getColor8().a) end
        end
        return table.concat(data)
    end
    if body_table then
        local function measure_body(phase, enabled)
            local view = open(assert(controller:getCssText()))
            local reference, body = geometry(view, "The harbor census")
            local table_page, first, first_pointer = geometry(view, "Well list")
            local inlet_page, inlet, inlet_pointer = geometry(view, "Eastern inlet")
            local beacon_page, beacon, beacon_pointer = geometry(view, "Red beacon")
            local gate_page, gate, gate_pointer = geometry(view, "Gate stays open")
            local note, _, note_pointer = geometry(view, "The gauge was calibrated")
            local backlink_page, backlink = geometry(view, "Return to the census")
            assert(table_page == inlet_page and inlet_page == beacon_page and beacon_page == gate_page)
            assert(first.y0 == inlet.y0 and inlet.y0 < beacon.y0 and beacon.y0 < gate.y0)
            assert(math.abs(first.x0 - gate.x0) <= 2 and math.abs(inlet.x0 - beacon.x0) <= 2 and first.x0 < inlet.x0,
                ("cell starts x=%d,%d,%d,%d"):format(first.x0, inlet.x0, beacon.x0, gate.x0))
            assert(note > table_page and note == backlink_page)
            assert(view:getLinkFromPosition(backlink.x0 + 6, math.floor((backlink.y0 + backlink.y1)/2)):match(" body%-grid%-ref$"))
            if enabled then
                assert(table_page == reference + 1)
                assert(capture(view, reference, "stock-body-table-notes-band"))
                assert(not capture(view, table_page, "stock-body-table-notes-grid"))
            else
                assert(table_page == reference)
                local name = phase == "off" and "stock-body-table-notes-off" or "stock-body-table-notes-restored"
                assert(not capture(view, reference, name))
            end
            view:gotoPage(table_page)
            local x, y = first.x0 + 6, math.floor((first.y0 + first.y1)/2)
            assert(view:getLinkFromPosition(x, y):match(" body%-grid%-target$"))
            assert(view:getTextFromPositions(x, y, x, y, false, false).text:find("Well", 1, true))
            local frame = draw(view, table_page)
            local x0, x1, y0, y1 = outlined_rows(frame, math.max(margin, first.y0 - 12),
                math.min(height - margin - 1, gate.y1 + 12))
            local pixels = raster(frame, x0, x1, y0, y1)
            assert(pixels:find(string.char(221), 1, true) and pixels:find(string.char(238), 1, true))
            print(("mixed table %.0fx%.0f x=%d y=%d page-offset=%d"):format(x1-x0, y1-y0, x0, y0, table_page-reference))
            frame:free()
            view:close()
            return table.concat({body.y0, x0, x1, first_pointer, inlet_pointer, beacon_pointer,
                gate_pointer, note_pointer}, "|"), pixels
        end
        assert(row.checked_func())
        row.callback()
        assert(not row.checked_func())
        local before, raster_before = measure_body("off", false)
        row.callback()
        assert(row.checked_func())
        local after, raster_after = measure_body("on", true)
        assert(after == before and raster_after == raster_before)
        row.callback()
        assert(not row.checked_func())
        local restored, raster_restored = measure_body("restored", false)
        assert(restored == before and raster_restored == raster_before)
        print("Pinned body table stays intact while the preceding footnote gets a reference band")
        return
    end
    local function measure(phase, inline)
        local view = open(assert(controller:getCssText()))
        local reference, body = geometry(view, "The crossing report")
        local original, head = geometry(view, "Grid begins")
        local first_page, first, first_pointer = geometry(view, "Shoal ledger")
        local second_page, second, second_pointer = geometry(view, "Deep chart")
        local last_page, last, last_pointer = geometry(view, "Ferry count")
        local tail_page, tail = geometry(view, "Grid ends")
        assert(original > reference and original == first_page and original == second_page)
        assert(original == last_page and original == tail_page)
        assert(first.x0 < second.x0 and first.y0 == second.y0)
        if inline then
            assert(head.y0 == first.y0 and first.y0 == last.y0 and tail.y0 == last.y0)
            assert(head.x0 < first.x0 and second.x0 < last.x0 and last.x0 < tail.x0)
            assert(not capture(view, original, "stock-note-inline-table-original"))
        else
            assert(head.y0 < first.y0 and first.y0 < last.y0 and last.y0 < tail.y0)
            assert(first.x0 == last.x0)
        end
        local separator = assert(capture(view, reference, "stock-note-inline-table-" .. phase))
        local canonical_frame = draw(view, original)
        local ref_frame = draw(view, reference)
        local x0, x1, y0, y1 = outlined_rows(canonical_frame, head.y0, tail.y1)
        local a, b, c, d = outlined_rows(ref_frame, separator + 1, height - margin - 1)
        assert(x0 == a and x1 == b and y1 - y0 == d - c)
        local canonical_pixels = raster(canonical_frame, x0, x1, y0, y1)
        assert(canonical_pixels:find(string.char(221), 1, true) and canonical_pixels:find(string.char(238), 1, true))
        assert(canonical_pixels == raster(ref_frame, a, b, c, d))
        canonical_frame:free()
        ref_frame:free()
        local x = first.x0 + 6
        local y = math.floor((first.y0 + first.y1) / 2)
        assert(view:getLinkFromPosition(x, y + c - y0) == "")
        local copied = view:getTextFromPositions(x, y + c - y0, x, y + c - y0, false, false)
        assert(not copied or not copied.text:find("Shoal", 1, true))
        view:gotoPage(original)
        assert(view:getLinkFromPosition(x, y):match(" grid%-ref$"))
        local selected = assert(view:getTextFromPositions(x, y, x, y, false, false))
        assert(selected.text:find("Shoal", 1, true))
        print(("table=%.0fx%.0f canonical=(%d,%d) strip=(%d,%d)"):format(x1-x0, y1-y0, x0, y0, a, c))
        view:close()
        return table.concat({body.y0, first_pointer, second_pointer, last_pointer}, "|")
    end
    assert(not option.checked_func())
    local before = measure("before", false)
    option.callback()
    assert(option.checked_func())
    assert(measure("after", true) == before)
    option.callback()
    assert(not option.checked_func())
    assert(measure("restored", false) == before)
    print("Pinned EPUB note keeps the rendered table and its source-cell addresses")
    return
end

if arg[5] == "cosmetic" then
    controller:onReadSettings(Settings:wrap({}))
    local option = assert(find_tweak(menu.style_tweaks.sub_item_table,
        "inpage_footnote_no_inline_spacing"))
    local function measure(name)
        local view = open(assert(controller:getCssText()))
        local ref = position(view, "The survey record")
        assert(capture(view, ref, "stock-note-cosmetic-" .. name))
        local _, note = width_in_canonical_note(view, "reads true")
        local _, body = width_in_canonical_note(view, "continues today")
        view:close()
        return note, body
    end
    assert(not option.checked_func())
    local before, body_before = measure("before")
    option.callback()
    assert(option.checked_func())
    local after, body_after = measure("after")
    assert(before - after >= 70 and body_before == body_after)
    print(string.format("Stock cosmetic note spacing x=%d -> %d; body x=%d", before, after, body_after))
    return
end

if wikipedia then
    local view = open(css)
    local ref = position(view, "The harbor reference")
    local original = position(view, "First printed register")
    assert(original > ref)
    position(view, "Second printed register")
    position(view, "Nested label stays")
    position(view, "The appendix is ordinary text")
    assert(not view:findAllText("Print return one", false, false, 8, true, 0, 255))
    assert(not view:findAllText("Print return two", false, false, 8, true, 0, 255))
    assert(capture(view, ref, "stock-wikipedia-note-band"))
    assert(not capture(view, original, "stock-wikipedia-note-canonical"))
    view:close()
    local ordinary = open("")
    position(ordinary, "Print return one")
    position(ordinary, "Print return two")
    local page = position(ordinary, "The harbor reference")
    assert(not capture(ordinary, page, "stock-wikipedia-notes-off"))
    ordinary:close()
    print("Stock Wikipedia list notes, hidden direct backlinks, and nested text verified")
    return
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

if fb3_note then
    controller:onReadSettings(Settings:wrap({}))
    local menu_items = menu.style_tweaks.sub_item_table
    local regular = assert(find_tweak(menu_items, "fb2_footnotes_regular_font_size"))
    local comments = assert(find_tweak(menu_items, "footnote-inpage_fb2_comments"))
    local epub = assert(find_tweak(menu_items, "footnote-inpage_epub"))
    local prose, note, backlink = "At dawn Mara checks the", "The spring tide peaks", "Return to soundings"
    local pointers = {}
    local function inspect(view, phrase)
        local page, y = position(view, phrase)
        local hit = view:findAllText(phrase, false, false, 8, true, 0, 255)[1]
        assert(not pointers[phrase] or hit.start == pointers[phrase], phrase)
        pointers[phrase] = hit.start
        return page, y, hit
    end
    local function render(name, band)
        local view = open(assert(controller:getCssText()))
        assert(view:getDocumentFormat():find("FB3", 1, true))
        local reference, y = inspect(view, prose)
        local canonical = inspect(view, note)
        local link_page, _, link = inspect(view, backlink)
        assert(reference < canonical and link_page == canonical)
        assert((capture(view, reference, "stock-fb3-note-" .. name) ~= nil) == band)
        if name == "on" then
            assert(not capture(view, canonical, "stock-fb3-note-original"))
        end
        view:gotoPage(canonical)
        local word = assert(view:getWordBoxesFromPositions(link.start, link["end"], true)[1])
        local href = view:getLinkFromPosition(word.x0 + 4, math.floor((word.y0 + word.y1) / 2))
        assert(href == "#morning", href)
        local note_width = width_in_canonical_note(view, "midnight")
        view:close()
        return note_width, y
    end
    assert(row.checked_func())
    local small, y = render("on", true)
    regular.callback()
    assert(regular.checked_func())
    local full, regular_y = render("regular", true)
    assert(small / full > 0.70 and small / full < 0.80 and regular_y == y)
    regular.callback()
    row.callback()
    assert(not regular.checked_func() and not row.checked_func())
    local ordinary, ordinary_y = render("off", false)
    assert(ordinary == full and ordinary_y == y)
    comments.callback()
    assert(comments.checked_func() and not row.checked_func())
    assert(render("comments-only", false) == full)
    comments.callback()
    epub.callback()
    assert(epub.checked_func() and not row.checked_func())
    assert(render("epub-only", false) == full)
    print(string.format("Pinned FB3 width at 75%%=%dpx, regular=%dpx, body y=%d", small, full, y))
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
