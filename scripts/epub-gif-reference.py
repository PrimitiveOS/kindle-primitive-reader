#!/usr/bin/env python3
"""Record independent Pillow first-frame pixels and the spine locations of embedded GIFs."""
import argparse
import hashlib
import json
import posixpath
from urllib.parse import unquote, urlsplit
from io import BytesIO
from pathlib import Path
import xml.etree.ElementTree as ET
from zipfile import ZipFile

from PIL import Image

def archive_member(base, href):
    uri = urlsplit(href)
    if uri.scheme or uri.netloc or not uri.path:
        return None
    return posixpath.normpath(posixpath.join(posixpath.dirname(base), unquote(uri.path)))

def inventory(filename):
    with ZipFile(filename) as archive:
        container = ET.fromstring(archive.read("META-INF/container.xml"))
        opf = container.find(".//{*}rootfile").attrib["full-path"]
        package = ET.fromstring(archive.read(opf))
        manifest = {item.get("id"): (archive_member(opf, item.get("href")), item.get("media-type")) for item in package.findall(".//{*}manifest/{*}item")}
        assets = {}
        for member, media in manifest.values():
            if media != "image/gif":
                continue
            data = archive.read(member)
            with Image.open(BytesIO(data)) as image:
                assert image.format == "GIF"
                rgba = image.convert("RGBA")
                gray = bytes(255 if a == 0 else (2126*r + 7152*g + 722*b)//10000 for r,g,b,a in rgba.getdata())
                assets[member] = {"width":image.width,"height":image.height,"frames":image.n_frames,"md5":hashlib.md5(gray).hexdigest()}
        chapters = {}
        sample = None
        for chapter, item in enumerate(package.findall(".//{*}spine/{*}itemref")):
            entry, _ = manifest[item.get("idref")]
            root = ET.fromstring(archive.read(entry))
            refs = set()
            parents = {child:parent for parent in root.iter() for child in parent}
            for node in root.iter():
                tag=node.tag.split("}")[-1]
                if tag not in ("img","image","object"): continue
                href = node.get("src") or node.get("href") or node.get("{http://www.w3.org/1999/xlink}href") or node.get("data")
                member = archive_member(entry, href) if href else None
                if member not in assets: continue
                refs.add(member)
                area = assets[member]["width"] * assets[member]["height"]
                if tag == "img" and (not sample or sample["area"] < area):
                    parts = []
                    current = node
                    while current in parents:
                        label = current.tag.split("}")[-1]
                        siblings = [sibling for sibling in parents[current] if sibling.tag == current.tag]
                        parts.insert(0, label + (f"[{siblings.index(current)+1}]" if len(siblings) > 1 else ""))
                        if label == "body": break
                        current = parents[current]
                    sample = {"area": area, "member":member, "xpointer": f"/body/DocFragment[{chapter+1}]/" + "/".join(parts)}
            if refs: chapters[chapter]=sorted(refs)
    return {"assets":assets,"chapters":chapters,"sample":sample}

if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("publication", type=Path)
    cli.add_argument("output", type=Path)
    args=cli.parse_args()
    data=inventory(args.publication)
    args.output.write_text(json.dumps(data,indent=2)+"\n")
    print(f"{len(data['assets'])} GIF resources, {len(data['chapters'])} sections with inline GIF references")
