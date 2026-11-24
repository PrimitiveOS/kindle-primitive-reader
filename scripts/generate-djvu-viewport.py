#!/usr/bin/env python3
"""Encode Reader's original Signal Log plate and its selectable text."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 480, 640


def author(folder):
    image = Image.new('1', (WIDTH, HEIGHT), 1)
    ink = ImageDraw.Draw(image)
    font = ROOT / 'tests/fixtures/fonts/NotoSans-Regular.ttf'
    title = ImageFont.truetype(str(font), 35)
    label = ImageFont.truetype(str(font), 20)
    lines = []

    def text(x, y, face, phrase):
        left = x
        words = []
        for word in phrase.split():
            ink.text((x, y), word, font=face, fill=0)
            x0, y0, x1, y1 = ink.textbbox((x, y), word, font=face)
            words.append(f'(word {x0} {HEIGHT - y1} {x1} {HEIGHT - y0} {json.dumps(word)})')
            x += round(ink.textlength(word + ' ', font=face))
        _, y0, right, y1 = ink.textbbox((left, y), phrase, font=face)
        lines.append(f'(line {left} {HEIGHT - y1} {right} {HEIGHT - y0} ' + ' '.join(words) + ')')

    text(74, 105, title, 'Signal log')
    ink.line((69, 171, 409, 171), fill=0, width=2)
    ink.rectangle((74, 222, 405, 419), outline=0, width=2)
    ink.line([(94, 383), (146, 319), (181, 361), (253, 282), (284, 307), (380, 245)], fill=0, width=3)
    ink.rectangle((247, 276, 261, 290), fill=0)
    ink.rectangle((193, 309, 308, 343), fill=1, outline=0, width=2)
    text(205, 313, label, 'BEACON')
    text(76, 447, label, 'Outer shoal')
    image.save(folder / 'signal.pbm')
    (folder / 'words.txt').write_text(f'(page 0 0 {WIDTH} {HEIGHT} ' + ' '.join(lines) + ')\n')
    subprocess.run(['cjb2', '-dpi', '150', 'signal.pbm', 'signal-log.djvu'], cwd=folder, check=True)
    subprocess.run(['djvused', '-s', '-e', 'select 1; set-txt words.txt', 'signal-log.djvu'], cwd=folder, check=True)
    shutil.copyfile(folder / 'signal-log.djvu', ROOT / 'tests/fixtures/signal-log.djvu')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory() as work:
        author(Path(work))
