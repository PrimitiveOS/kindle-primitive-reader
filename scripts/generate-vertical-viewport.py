#!/usr/bin/env python3
"""Generate the original bilingual vertical EPUB with a small OFL font subset."""
import argparse
from io import BytesIO
from pathlib import Path
import runpy

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[1]
FONT_NAME = 'Harbor Vertical Test Gothic'
FONT_POSTSCRIPT = 'HarborVerticalTestGothic-Regular'
CHAPTERS = [
    ('japanese', 'ja', '雨の港', '灯台', '夜明けの港を歩いた。潮の匂いがした。',
     '見上げると、', '。雨の向こうで光っていた。', 'chinese'),
    ('chinese', 'zh-Hant', '潮汐手記', '港口', '夜裡的潮水緩緩升起。海面很安靜。',
     '小船回到', '。岸邊的燈仍然亮著。', 'japanese'),
]


def embedded_font(path, text):
    with TTFont(path, recalcTimestamp=False) as font:
        options = subset.Options()
        options.layout_features = ['*']
        options.name_IDs = ['*']
        options.name_languages = ['*']
        worker = subset.Subsetter(options)
        worker.populate(text=text)
        worker.subset(font)
        for record in font['name'].names:
            names = {1: FONT_NAME, 3: FONT_POSTSCRIPT, 4: FONT_NAME,
                     6: FONT_POSTSCRIPT, 16: FONT_NAME, 21: FONT_NAME}
            if record.nameID in names:
                record.string = names[record.nameID].encode(record.getEncoding())
        cff = font['CFF '].cff
        cff.fontNames = [FONT_POSTSCRIPT]
        for top in cff.topDictIndex:
            top.FamilyName = FONT_NAME
            top.FullName = FONT_NAME
        out = BytesIO()
        font.save(out)
        return out.getvalue()


def generate(font_path, license_path):
    helpers = runpy.run_path(str(ROOT / 'scripts/generate-fixtures.py'))
    xhtml = helpers['xhtml']
    chapters = []
    sample = ' '  # CREngine requires U+0020 when registering a font.
    for slug, language, title, linked, paragraph, before, after, target in CHAPTERS:
        text = before + linked + after
        sample += title + text + paragraph
        lines = [f'<h1 id="landing">{title}</h1>']
        for number in range(1, 39):
            if number in (3, 24):
                key = 'earlier' if number == 3 else 'later'
                lines.append(f'<p>{before}<a id="{key}" href="{target}.xhtml#landing">{linked}</a>{after}</p>')
            else:
                lines.append(f'<p>{paragraph}</p>')
        chapter = xhtml(title, '\n'.join(lines), True, '../styles/vertical.css')
        chapter = chapter.replace('xml:lang="en"', f'xml:lang="{language}"', 1)
        chapters.append((f'EPUB/chapters/{slug}.xhtml', title, chapter))
    long_base = ('夜明けの港を歩いた潮の匂いがした' * 2)[:25]
    ruby = 'よあけ'
    sample += long_base + ruby
    chapter = xhtml('Ruby bounds', f'<p id="ruby"><ruby>{long_base}<rt>{ruby}</rt></ruby></p>',
                    True, '../styles/vertical.css')
    chapter = chapter.replace('xml:lang="en"', 'xml:lang="ja"', 1)
    chapters.append(('EPUB/chapters/ruby.xhtml', 'Ruby bounds', chapter))
    stylesheet = f'''@font-face {{ font-family: "{FONT_NAME}"; src: url("../fonts/vertical-gothic.otf"); }}
body {{ font-family: "{FONT_NAME}", sans-serif; writing-mode: vertical-rl; }}
'''
    entries = helpers['publication']('Harbor in Rain', 'harbor-vertical-viewport', 3, chapters, [
        ('EPUB/styles/vertical.css', 'text/css', stylesheet),
        ('EPUB/fonts/vertical-gothic.otf', 'font/otf', embedded_font(font_path, sample)),
    ], rtl=True)
    entries['META-INF/font-license.txt'] = license_path.read_bytes()
    return helpers['archive'](entries)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('font', type=Path, help='Noto Sans CJK JP Regular OpenType font')
    parser.add_argument('license', type=Path, help='The accompanying SIL Open Font License')
    options = parser.parse_args()
    fixture = ROOT / 'tests/fixtures/harbor-vertical-viewport.epub'
    fixture.write_bytes(generate(options.font, options.license))
