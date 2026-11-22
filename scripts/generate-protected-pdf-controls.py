#!/usr/bin/env python3
"""Build the authored protected-PDF test set with pypdf and cryptography."""

import argparse
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import DecodedStreamObject, DictionaryObject, NameObject

OWNER = "survey-editor"
USER = "survey-entry"
LONG = "estuary" + "n" * 88 + "river"
CASES = [
    ("r2", "RC4-40", USER),
    ("r3", "RC4-128", USER),
    ("r4", "AES-128", USER),
    ("r5", "AES-256-R5", USER),
    ("r6", "AES-256", USER),
    ("r6-long", "AES-256", LONG),
    ("r4-owner-only", "AES-128", ""),
    ("r4-latin", "AES-128", "été"),
    ("r6-latin", "AES-256", "été"),
    ("plain", None, None),
]


def create(path, algorithm, user, owner):
    writer = PdfWriter()
    font = DictionaryObject({
        NameObject("/Type"): NameObject("/Font"),
        NameObject("/Subtype"): NameObject("/Type1"),
        NameObject("/BaseFont"): NameObject("/Helvetica"),
    })
    font = writer._add_object(font)
    for i, title in enumerate(("Opening", "Wind survey", "Reading trail")):
        page = writer.add_blank_page(width=320, height=480)
        page[NameObject("/Resources")] = DictionaryObject({
            NameObject("/Font"): DictionaryObject({NameObject("/F1"): font})
        })
        stream = DecodedStreamObject()
        stream.set_data(f"BT /F1 18 Tf 35 392 Td ({title}) Tj 0 -40 Td (Field notes, page {i+1}.) Tj ET".encode("ascii"))
        page[NameObject("/Contents")] = writer._add_object(stream)
        writer.add_outline_item(title=title, page_number=i)
    if algorithm:
        writer.encrypt(user, owner, algorithm=algorithm)
    writer.write(path)
    if algorithm:
        for credential in (user, owner):
            reader = PdfReader(path)
            assert reader.decrypt(credential)
            assert "Wind survey" in reader.pages[1].extract_text()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="directory for the authored PDFs")
    output = parser.parse_args().output
    output.mkdir(parents=True, exist_ok=True)
    for stem, algorithm, user in CASES:
        create(output / (stem + ".pdf"), algorithm, user, "côté" if stem.endswith("-latin") else OWNER)
