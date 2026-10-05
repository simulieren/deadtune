"""Print every rule of a stylesheet whose selector contains one of the given needles.
usage: rules.py <file.css> needle [needle...]"""
import sys

from cssdiff import parse

defs, rules = parse(open(sys.argv[1]).read())
needles = sys.argv[2:]
for sel, decls in rules:
    if any(n in sel for n in needles):
        body = " ".join(f"{p}:{v};" for p, v in decls)
        print(f"{sel} {{ {body} }}")
