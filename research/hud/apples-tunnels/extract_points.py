#!/usr/bin/env python3
"""Prints the apple and tunnel entrance tables for crates/dt-core/src/hud/apples_tunnels.rs
from FesamAyt's decoded minimap script (apple_snacks_tunnel_map.vjs_c, DATA block as text).

Usage: extract_points.py <decoded .js>
Only the coordinates are taken (factual map data); none of the mod's code or files ship.
"""
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()


def table(var, const):
    raw = re.search(r"var %s = (\[.*?\]);" % var, text).group(1)
    points = json.loads(raw)
    print("pub static %s: [MapPoint; %d] = [" % (const, len(points)))
    for p in points:
        print("    MapPoint::new(%r, %r)," % (float(p["u"]), float(p["v"])))
    print("];")


table("applePoints", "APPLES")
table("points", "TUNNEL_ENTRANCES")
