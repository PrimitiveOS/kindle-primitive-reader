require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Registry = require("document/documentregistry")
local provider = Registry:getProviderFromKey("mupdf")
local cases = {
    {"spacing", "line_spacing", 1.0}, {"spacing", "line_spacing", 1.2}, {"spacing", "line_spacing", 1.4},
    {"align", "justification", -1}, {"align", "justification", 0}, {"align", "justification", 1},
    {"align", "justification", 2}, {"align", "justification", 3},
    {"gap", "word_spacing", 0.05}, {"gap", "word_spacing", -0.2}, {"gap", "word_spacing", 0.375},
    {"scale", "font_size", 0.5}, {"scale", "font_size", 1.0}, {"scale", "font_size", 1.6},
}

for index = 1, #arg do
    local doc = assert(Registry:openDocument(arg[index], provider))
    local config = doc.configurable
    config.text_wrap = 1
    config.trim_page = 3
    config.max_columns = 1
    config.auto_straighten = 0
    config.background_cleanup = 0
    doc.bbox = { [1] = { x0 = 0, y0 = 0, x1 = 600, y1 = 800 } }
    local pitch, spacing_shape = {}, nil
    local gaps, scales, alignment = {}, {}, {}
    for _, case in ipairs(cases) do
        config.line_spacing = 1.2
        config.justification = 0
        config.word_spacing = -0.2
        config.font_size = (case[1] == "align" or case[1] == "gap") and 1.6 or 1.0
        config[case[2]] = case[3]
        local kc = doc.koptinterface:getCachedContext(doc, 1)
        local width, height = kc:getPageDim()
        local lines, words = kc:getReflowedWordBoxes("dst", 0, 0, width, height)
        assert(lines and #lines >= 2 and words >= #lines)
        local first, second = lines[1], lines[2]
        local distinct_gaps, total, max_floor, left = 0, 0, 0, 0
        for _, line in ipairs(lines) do
            max_floor = math.max(max_floor,line.y1)
            left = left + line.x0
            for k = 2, #line do
                total = total + line[k].x0 - line[k-1].x1
                distinct_gaps = distinct_gaps + 1
            end
        end
        local first_gap = distinct_gaps > 0 and total / distinct_gaps or -1
        if case[1] == "spacing" then
            local shape = string.format("%d:%d:%d:%d:%d", #lines, words,
                first.x0, first.x1 - first.x0, first.y1 - first.y0)
            assert(not spacing_shape or shape == spacing_shape)
            spacing_shape = shape
            pitch[#pitch + 1] = second.y0 - first.y0
        elseif case[1] == "gap" then
            gaps[case[3]] = #lines
        elseif case[1] == "scale" then
            scales[case[3]] = first.y1 - first.y0
        else
            alignment[case[3]] = words
        end
        print(string.format("FLOW %d %s %.3f canvas=%dx%d lines=%d words=%d first=(%d,%d,%d,%d) gap_y=%d mean_gap_x=%.2f mean_left=%.2f floor=%d",
            index, case[1], case[3], width, height, #lines, words, first.x0, first.y0, first.x1, first.y1,
            second.y0 - first.y0, first_gap,left/#lines,max_floor))
    end
    assert(pitch[1] < pitch[2] and pitch[2] < pitch[3])
    assert(math.abs(pitch[1] / pitch[2] - 1.0 / 1.2) < 0.035)
    assert(math.abs(pitch[3] / pitch[2] - 1.4 / 1.2) < 0.035)
    assert(gaps[0.05] < gaps[-0.2] and gaps[-0.2] == gaps[0.375])
    assert(scales[0.5] < scales[1] and scales[1] < scales[1.6])
    assert(alignment[3] > alignment[0])
    doc:close()
end
Device:exit()
