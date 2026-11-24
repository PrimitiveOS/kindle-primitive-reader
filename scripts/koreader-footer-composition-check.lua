-- Run inside the pinned KOReader Mac runtime with a scratch KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)

local ReaderFooter = require("apps/reader/modules/readerfooter")
local TextWidget = require("ui/widget/textwidget")
local Font = require("ui/font")
local generators
for index = 1, 20 do
    local name, value = debug.getupvalue(ReaderFooter.updateFooterTextGenerator, index)
    if name == "footerTextGeneratorMap" then
        generators = value
        break
    end
end
assert(generators, "The stock footer generator table is unavailable")

local book_title = "Field guide to the coast"
local settings = {
    chapter_progress = true,
    dynamic_filler = true,
    book_title = true,
    book_title_max_width_pct = 90,
    all_at_once = true,
    item_prefix = "letters",
    items_separator = "bar",
    progress_bar_position = "below",
    align = "left",
    text_font_bold = true,
}
local pages_done, chapter_pages, hidden, page_in_flow
local footer = setmetatable({
    mode_index = {"chapter_progress", "dynamic_filler", "book_title"},
    pageno = 8,
    pages = 12,
    settings = settings,
    _saved_screen_width = 760,
    horizontal_margin = 24,
    footer_text_face = Font:getFace("cfont", 14),
    ui = {
        doc_props = {display_title = book_title},
        toc = {
            getChapterPagesDone = function() return pages_done end,
            getChapterPageCount = function() return chapter_pages end,
        },
        document = {
            hasHiddenFlows = function() return hidden end,
            getPageNumberInFlow = function() return page_in_flow end,
        },
    },
}, {__index = ReaderFooter})

assert(footer:getChapterProgress() == "8 ⁄⁄ 12")
pages_done, chapter_pages = 1, 4
assert(generators.chapter_progress(footer) == "2 ⁄⁄ 4")
assert(footer:getChapterProgress(true) == 0.5)
hidden, page_in_flow, pages_done = true, 3, nil
assert(footer:getChapterProgress() == "3 ⁄⁄ 4")
hidden, pages_done = false, 1

local function width(text)
    local widget = TextWidget:new{
        text = text,
        face = footer.footer_text_face,
        bold = settings.text_font_bold,
    }
    local result = widget:getSize().w
    widget:free()
    return result
end
assert(footer:updateFooterTextGenerator())
local plain, filler_inside = footer:genAllFooterText(generators.dynamic_filler)
assert(filler_inside and plain:find("|", 1, true))
local filler, merge = generators.dynamic_filler(footer)
assert(merge and filler:match("^ +$"))
local result = footer:genFooterText()
assert(not result:find("|", 1, true))
assert(result:find("2 ⁄⁄ 4", 1, true))
assert(result:find(filler, 1, true))
local max_width = footer._saved_screen_width - 2 * footer.horizontal_margin
assert(#filler == math.floor((max_width - width(plain) + width(" | ")) / width(" ")))
local portrait_filler = #filler

footer._saved_screen_width = 1080
filler = assert(generators.dynamic_filler(footer))
assert(#filler > portrait_filler)
settings.progress_bar_position = "alongside"
assert(generators.dynamic_filler(footer) == nil)
assert(footer:genFooterText():find("|", 1, true))
settings.disable_progress_bar = true
assert(generators.dynamic_filler(footer))
assert(not footer:genFooterText():find("|", 1, true))

footer.mode_index = {"dynamic_filler", "chapter_progress", "book_title"}
assert(footer:updateFooterTextGenerator())
local _, inside_at_start = footer:genAllFooterText(generators.dynamic_filler)
assert(not inside_at_start)
assert(footer:genFooterText():match("^ "))
footer.mode_index = {"chapter_progress", "book_title", "dynamic_filler"}
assert(footer:updateFooterTextGenerator())
local _, inside_at_end = footer:genAllFooterText(generators.dynamic_filler)
assert(not inside_at_end)
assert(footer:genFooterText():match(" $"))
print("Stock chapter counts, measured interior and edge filler, and alongside-track fallback verified")
