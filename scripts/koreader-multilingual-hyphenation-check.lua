-- Run with the pinned native KOReader runtime and the named XHTML specimen.
-- Offsets are Unicode scalars before a visual line break, as counted by CRE XPointers.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
require("document/canvascontext"):init(require("device"))
local CreDocument = require("document/credocument")
local Cre = CreDocument:engineInit()
local BB = require("ffi/blitbuffer")
local book, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local specimen = arg[5] or "multilingual-hyphenation"
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
Cre.initHyphDict("./data/hyph/")

local samples = {
    { "es-ES", "información",          "2,5,7",           "5,7" },
    { "es-ES", "lingüística",          "3,7,9",           "7" },
    { "es-ES", "murciélago",           "3,6,8",           "6" },
    { "it-IT", "straordinario",        "6,8,10",          "6,8" },
    { "it-IT", "elettroencefalogramma", "4,9,11,13,15,19", "4,9,11,13,15" },
    { "it-IT", "acqua",                "2",               "" },
    { "pt-PT", "informação",           "2,5,7",           "5" },
    { "pt-PT", "linguística",          "3,7",             "7" },
    { "pt-PT", "extraordinário",       "2,5,7,9,11",      "5,7,9" },
    { "pt-PT", "saúde",                "",                "" },
}
local dictionaries = {
    { "es-ES", "Spanish.pattern",    4692, 2 },
    { "it-IT", "Italian.pattern",      326, 2 },
    { "pt-PT", "Portuguese.pattern",   308, 3 },
}
if specimen == "estonian-finnish-hyphenation" then
    samples = {
        { "et-EE", "õigekeelsussõnaraamat", "2,4,8,11,13,15,18", "4,8,11,13,15" },
        { "et-EE", "raudteejaam",          "4,7",               "4,7" },
        { "et-EE", "lühiülevaade",         "4,7",               "4,7" },
        { "et-EE", "päikesepaisteline",    "3,5,7,11,13",       "5,7,11,13" },
        { "fi-FI", "työskenteleminen",     "4,7,9,11,13",       "4,7,9,11" },
        { "fi-FI", "ääntäminen",           "3,5,7",             "5" },
        { "fi-FI", "kansainvälinen",       "3,7,9,11",          "7,9" },
        { "fi-FI", "taloudellinen",        "2,5,8,10",          "5,8" },
        { "fi-FI", "kalastus",             "2,5",               "" },
    }
    dictionaries = {
        { "et-EE", "Estonian.pattern", 3691, 3 },
        { "fi-FI", "Finnish.pattern",   285, 2 },
    }
elseif specimen == "danish-dutch-hyphenation" then
    samples = {
        { "da-DK", "læseoplevelse",        "2,4,6,8,11",       "4,6,8",         "læ-se-op-le-vel-se",       "læse-op-le-velse" },
        { "da-DK", "bibliotekernes",      "5,6,8,11",         "5,6,8",         "bi-bli-o-te-ker-nes",      "bibli-o-te-kernes" },
        { "da-DK", "samarbejdsmuligheder", "3,5,10,12,15,17",  "5,10,12,15",    "sam-ar-bejds-mu-lig-he-der", "samar-bejds-mu-lig-heder" },
        { "da-DK", "påmindelser",         "2,5,8",            "5",             "på-min-del-ser",          "påmin-delser" },
        { "da-DK", "færgeforbindelse",    "3,5,8,11,14",      "5,8,11",        "fær-ge-for-bin-del-se",    "færge-for-bin-delse" },
        { "nl-NL", "coördinatie",         "2,4,6,8",          "4,6",           "co-ör-di-na-tie",          "coör-di-natie" },
        { "nl-NL", "bibliotheek",         "5,6",              "5,6",           "bi-bli-o-theek",           "bibli-o-theek" },
        { "nl-NL", "schilderstalent",     "5,8,9,11",         "5,8,9,11",      "schil-der-s-ta-lent",      "schil-der-s-ta-lent" },
        { "nl-NL", "waterstaatsingenieur","2,5,10,11,13,15,17","5,10,11,13,15","wa-ter-staat-s-in-ge-ni-eur", "water-staat-s-in-ge-nieur" },
        { "nl-NL", "maatschappij",        "4,9",              "4",             "maat-schap-pij",           "maat-schappij" },
    }
    dictionaries = {
        { "da-DK", "Danish.pattern",  1144, 2 },
        { "nl-NL", "Dutch.pattern",  12764, 2 },
    }
else
    assert(specimen == "multilingual-hyphenation", specimen)
end
local initial_width, height, margin = 960, 864, 16
local view = Cre.newDocView(initial_width, height, 1)
view:setStyleSheet("./data/epub.css", "")
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(margin, margin, 18, 18)
view:setFontFace("Noto Sans")
view:setFontSize(24)
local doc = setmetatable({ _document = view, file = book }, { __index = CreDocument })
doc:setTextMainLang("en-US")
doc:setTextEmbeddedLangs(true)
doc:setTextHyphenation(true)
doc:setTextHyphenationSoftHyphensOnly(false)
doc:setTextHyphenationForceAlgorithmic(false)
doc:setTrustSoftHyphens(false)
doc:setHyphLeftHyphenMin(0)
doc:setHyphRightHyphenMin(0)
assert(view:loadDocument(book, false))
view:renderDocument()

local main, active, loaded = Cre.getTextLangStatus()
assert(main == "en-US" and active == "English_US.pattern")
for _, dictionary in ipairs(dictionaries) do
    local language, file, count, last = unpack(dictionary)
    assert(loaded[language].hyph_dict_name == file)
    assert(loaded[language].hyph_nb_patterns == count)
    doc:setTextMainLang(language)
    local _, left, right = Cre.getSelectedHyphDict()
    assert(left == 2 and right == last)
    print(("%s %s, %d patterns, minima %d/%d"):format(language, file, count, left, right))
end
if specimen == "estonian-finnish-hyphenation" then
    doc:setTextMainLang("et-EE")
    local candidates = Cre.getHyphenationForWord("lühiülevaade")
    assert(candidates == "lü-hi-üle-vaade", candidates)
    print("dictionary candidates", "et-EE", candidates)
end
doc:setTextMainLang("en-US")

local function hits(text)
    return assert(view:findAllText(text, false, false, 20, true, 0, 255))
end

local prefixes = hits("These notes show how the last word wraps:")
assert(#prefixes == #samples)
local words, min_width, max_width, max_word_width = {}, 0, 0, 0
for i, sample in ipairs(samples) do
    local found = hits(sample[2])
    assert(#found == 1)
    local hit = found[1]
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == sample[2])
    local stem, length = hit["end"]:match("^(.-)%.(%d+)$")
    assert(stem and hit.start == stem .. ".0")
    local prefix = prefixes[i].start
    assert(prefix:gsub("/span%[1%]/.*", "") == stem:gsub("/span%[2%]/.*", ""))
    local start_y, start_x = view:getPosFromXPointer(hit.start)
    local end_y, end_x = view:getPosFromXPointer(hit["end"])
    assert(start_y == end_y and start_y == view:getPosFromXPointer(prefix))
    words[i] = { prefix = prefix, stem = stem, length = tonumber(length) }
    min_width = math.max(min_width, start_x + 2 * margin)
    max_width = math.max(max_width, end_x + 2 * margin + 2)
    max_word_width = math.max(max_word_width, end_x - start_x)
end
-- Every complete sample word fits after the prefix moves to its own line.
-- Thus this sweep excludes forced breaks in words wider than the viewport.
assert(max_word_width < min_width - 2 * margin and max_width <= initial_width)

local function wrap(word)
    local y = view:getPosFromXPointer(word.stem .. ".0")
    if y ~= view:getPosFromXPointer(word.prefix) then return end
    for at = 1, word.length - 1 do
        if view:getPosFromXPointer(word.stem .. "." .. at) > y then return at end
    end
end

local function measure(label, enabled, minimum, expected_column)
    doc:setTextHyphenation(enabled)
    doc:setHyphLeftHyphenMin(minimum)
    doc:setHyphRightHyphenMin(minimum)
    if enabled and samples[1][5] then
        local candidate_column = minimum == 0 and 5 or 6
        for _, sample in ipairs(samples) do
            doc:setTextMainLang(sample[1])
            local candidate = Cre.getHyphenationForWord(sample[2])
            assert(candidate == sample[candidate_column], sample[2] .. ": " .. candidate)
            print("dictionary candidates", label, sample[1], candidate)
        end
        doc:setTextMainLang("en-US")
    end
    local seen, sample_width, best_score = {}, nil, 0
    for i = 1, #words do seen[i] = {} end
    for width = min_width, max_width do
        view:setViewDimen(width, height)
        view:renderDocument()
        local split_count, language_count, split_languages = 0, 0, {}
        for i, word in ipairs(words) do
            local at = wrap(word)
            if at then
                seen[i][at] = true
                split_count = split_count + 1
                local language = samples[i][1]
                if not split_languages[language] then
                    split_languages[language] = true
                    language_count = language_count + 1
                end
            end
        end
        local score = language_count * #words + split_count
        if score > best_score then
            sample_width, best_score = width, score
        end
    end
    for i, word in ipairs(words) do
        local offsets = {}
        for at = 1, word.length - 1 do
            if seen[i][at] then table.insert(offsets, at) end
        end
        local vector = table.concat(offsets, ",")
        local expected = expected_column and samples[i][expected_column] or ""
        assert(vector == expected, samples[i][2] .. ": " .. vector)
        print(label, samples[i][1], samples[i][2], vector ~= "" and vector or "none")
    end
    return sample_width
end

local function capture(width, filename)
    view:setViewDimen(width, height)
    view:renderDocument()
    local first = view:getPageFromXPointer(words[1].prefix)
    assert(view:getPageFromXPointer(words[#words].prefix) == first, "capture needs one page")
    view:gotoPage(first)
    local page = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(page, false, false, false, false)
    page:writePNG(output .. "/" .. filename .. ".png", false)
    page:free()
end

local sample_width = assert(measure("dictionary default", true, 0, 3))
capture(sample_width, "stock-" .. specimen)
measure("4/4 minima", true, 4, 4)
capture(sample_width, "stock-" .. specimen .. "-limits")
assert(not measure("hyphenation off", false, 0))
capture(sample_width, "stock-" .. specimen .. "-off")
print(("Comparison captures at %d × %d"):format(sample_width, height))
view:close()
