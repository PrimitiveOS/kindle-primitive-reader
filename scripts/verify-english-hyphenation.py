#!/usr/bin/env python3
"""Compare Reader's generated American English rules with pinned CREngine XML."""

import argparse
from collections import defaultdict
import hashlib
from pathlib import Path
import re
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[1]
RULES_SHA256 = "9185d734d23492ac9872496e617b2a8e6ef19612a74e8f5d9a8e56817ee8b817"
MATRIX_SHA256 = "f99458ab19c0bb9492c56ce10096922415ef05a2823d84826c0cf24d11b7cf7b"


def correction_words():
    words = set()
    for path in (ROOT / "assets/hyphenation").glob("*.tex"):
        tex = re.sub(r"%[^\n]*", "", path.read_text(encoding="utf-8"))
        entries = tex.split(r"\hyphenation{", 1)[1].split("}", 1)[0]
        words.update(word.replace("-", "").lower() for word in entries.split())
    return sorted(words)


def reference_weights(rules):
    patterns = defaultdict(list)
    for rule in rules:
        weights = [0]
        letters = ""
        for character in rule:
            if character.isdigit():
                weights[-1] = int(character)
            else:
                letters += character
                weights.append(0)
        patterns[letters].append(weights)
    return patterns


def candidate_offsets(word, patterns):
    letters = "." + word + "."
    scores = [0] * (len(letters) + 1)
    for start in range(len(letters)):
        for end in range(start + 1, len(letters) + 1):
            for weights in patterns.get(letters[start:end], ()):
                for delta, weight in enumerate(weights):
                    scores[start + delta] = max(scores[start + delta], weight)
    return [i for i in range(2, len(word) - 2) if scores[i + 1] % 2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path, help="CREngine b32a88ff English_US.pattern")
    parser.add_argument("generated", type=Path, help="Reader build output english_rules.txt")
    args = parser.parse_args()
    rules = sorted(node.text.replace(" ", ".") for node in
                   ElementTree.parse(args.reference).getroot().findall("pattern"))
    normalized = ("\n".join(rules) + "\n").encode("ascii")
    if hashlib.sha256(normalized).hexdigest() != RULES_SHA256:
        parser.error("The reference differs from the pinned US dictionary.")
    if normalized != args.generated.read_bytes():
        parser.error("The generated weighted rules differ from the reference.")
    patterns = reference_weights(rules)
    words = correction_words()
    matrix = "".join(f"{word} {','.join(map(str, candidate_offsets(word, patterns)))}\n"
                     for word in words).encode("ascii")
    if hashlib.sha256(matrix).hexdigest() != MATRIX_SHA256:
        parser.error("The resulting correction-word positions differ from the reference.")
    print(f"{len(rules):,} identical rules, {len(words):,} verified correction spellings")


if __name__ == "__main__":
    main()
