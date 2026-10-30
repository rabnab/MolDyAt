#!/usr/bin/env python3
"""Checks relative Markdown links and their anchors in docs/, README.md and
CONTRIBUTING.md. Prints one BROKEN or ANCHOR line per bad link and exits with
status 1 if there is any.

Usage: python3 scripts/check_doc_links.py   (from any directory)
"""
import glob
import os
import re
import sys

os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

files = glob.glob("docs/**/*.md", recursive=True) + ["README.md", "CONTRIBUTING.md"]


def anchors(path):
    with open(path, encoding="utf-8") as f:
        return {
            re.sub(r"[^\w\- ]", "", line.lstrip("#").strip().lower()).replace(" ", "-")
            for line in f
            if line.startswith("#")
        }


problems = 0
for f in files:
    with open(f, encoding="utf-8") as fh:
        text = fh.read()
    for m in re.finditer(r"\]\(([^)\s]+)\)", text):
        link = m.group(1)
        if link.startswith(("http", "mailto")):
            continue
        path, _, anchor = link.partition("#")
        target = os.path.normpath(os.path.join(os.path.dirname(f), path)) if path else f
        if not os.path.exists(target):
            print("BROKEN", f, link)
            problems += 1
        elif anchor and target.endswith(".md") and anchor not in anchors(target):
            print("ANCHOR", f, link)
            problems += 1

sys.exit(1 if problems else 0)
