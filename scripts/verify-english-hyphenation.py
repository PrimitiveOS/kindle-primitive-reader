#!/usr/bin/env python3
"""Compare Reader's generated English rules with pinned CREngine XML."""

import argparse
from collections import defaultdict
import hashlib
from pathlib import Path
import re
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[1]
VARIANTS = {
    "us": (
        ("hyph-en-us.tex", "ushyphex.tex"),
        "9185d734d23492ac9872496e617b2a8e6ef19612a74e8f5d9a8e56817ee8b817",
        "f99458ab19c0bb9492c56ce10096922415ef05a2823d84826c0cf24d11b7cf7b",
    ),
    "gb": (
        ("hyph-en-gb.tex",),
        "a0a2a0834a41bc7c0ce001de5c0d654b19a0475814b234d4dc4ed3d838b6499e",
        "0e81e36d9960a220400b86bd34829c9dd2924ddc7b6b65c7be24b1f084fc0ca8",
    ),
}


def correction_words(filenames):
    words = set()
    for name in filenames:
        path = ROOT / "assets/hyphenation" / name
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
    parser.add_argument("--variant", choices=VARIANTS, default="us")
    parser.add_argument("reference", type=Path, help="CREngine b32a88ff English_US.pattern or English_GB.pattern")
    parser.add_argument("generated", type=Path, help="Reader build output for the selected dictionary")
    args = parser.parse_args()
    filenames, rule_digest, matrix_digest = VARIANTS[args.variant]
    rules = sorted(node.text.replace(" ", ".") for node in
                   ElementTree.parse(args.reference).getroot().findall("pattern"))
    normalized = ("\n".join(rules) + "\n").encode("ascii")
    if hashlib.sha256(normalized).hexdigest() != rule_digest:
        parser.error("The reference differs from the selected pinned dictionary.")
    if normalized != args.generated.read_bytes():
        parser.error("The generated weighted rules differ from the reference.")
    patterns = reference_weights(rules)
    words = correction_words(filenames)
    matrix = "".join(f"{word} {','.join(map(str, candidate_offsets(word, patterns)))}\n"
                     for word in words).encode("ascii")
    if hashlib.sha256(matrix).hexdigest() != matrix_digest:
        parser.error("The resulting correction-word positions differ from the reference.")
    print(f"{len(rules):,} identical rules, {len(words):,} verified correction spellings")


if __name__ == "__main__":
    main()
