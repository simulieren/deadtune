"""Declaration-level diff of two Panorama stylesheets (prettified or minified).
usage: cssdiff.py <base.css> <mod.css>"""
import re
import sys


def strip_comments(t):
    return re.sub(r"/\*.*?\*/", "", t, flags=re.S)


def parse(text):
    """Returns (defines: dict, rules: list of (selector, [(prop, value)])) in order."""
    text = strip_comments(text)
    defines = {}
    rules = []
    i = 0
    n = len(text)
    while i < n:
        while i < n and text[i].isspace():
            i += 1
        if i >= n:
            break
        if text.startswith("@define", i) or text.startswith("@import", i):
            end = text.index(";", i)
            stmt = text[i:end].strip()
            if stmt.startswith("@define"):
                k, v = stmt[len("@define"):].split(":", 1)
                defines[k.strip()] = " ".join(v.split())
            else:
                defines[stmt] = ""
            i = end + 1
            continue
        brace = text.index("{", i)
        selector = " ".join(text[i:brace].split())
        selector = re.sub(r"\s*,\s*", ",", selector)
        if selector.startswith("@keyframes"):
            depth = 0
            j = brace
            while True:
                if text[j] == "{":
                    depth += 1
                elif text[j] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            body = text[brace + 1:j]
            rules.append((selector, [("<keyframes>", " ".join(body.split()))]))
            i = j + 1
            continue
        close = text.index("}", brace)
        body = text[brace + 1:close]
        decls = []
        for part in body.split(";"):
            if ":" not in part:
                continue
            p, v = part.split(":", 1)
            decls.append((p.strip(), " ".join(v.split())))
        rules.append((selector, decls))
        i = close + 1
    return defines, rules


def by_selector(rules):
    out = {}
    for sel, decls in rules:
        out.setdefault(sel, []).append(decls)
    return out


def flat(decls_list):
    out = {}
    for decls in decls_list:
        for p, v in decls:
            out[p] = v
    return out


def main():
    base_defs, base_rules = parse(open(sys.argv[1]).read())
    mod_defs, mod_rules = parse(open(sys.argv[2]).read())
    for k in sorted(set(base_defs) | set(mod_defs)):
        if base_defs.get(k) != mod_defs.get(k):
            print(f"DEFINE {k}: {base_defs.get(k)!r} -> {mod_defs.get(k)!r}")
    b = by_selector(base_rules)
    m = by_selector(mod_rules)
    for sel in m:
        if sel not in b:
            print(f"ADDED RULE {sel}")
            for decls in m[sel]:
                for p, v in decls:
                    print(f"    {p}: {v}")
    for sel in b:
        if sel not in m:
            print(f"REMOVED RULE {sel}")
            for decls in b[sel]:
                for p, v in decls:
                    print(f"    {p}: {v}")
    for sel in m:
        if sel in b:
            bf, mf = flat(b[sel]), flat(m[sel])
            if len(b[sel]) != len(m[sel]):
                print(f"COUNT {sel}: {len(b[sel])} -> {len(m[sel])} occurrences")
            for p in sorted(set(bf) | set(mf)):
                if bf.get(p) != mf.get(p):
                    print(f"CHANGED {sel} {{ {p}: {bf.get(p)!r} -> {mf.get(p)!r} }}")
    print(f"-- base {len(base_rules)} rules, mod {len(mod_rules)} rules")


if __name__ == "__main__":
    main()
