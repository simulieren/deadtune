import os
import sys
import urllib.request

RAW = "https://raw.githubusercontent.com/SteamTracking/GameTracking-Deadlock/{rev}/game/citadel/pak01_dir/{path}"
HERE = os.path.dirname(os.path.abspath(__file__))

FILES = [
    "panorama/layout/citadel_hud_top_bar.xml",
    "panorama/layout/citadel_hud_top_bar_player.xml",
    "panorama/layout/citadel_hud_top_bar_player_details.xml",
    "panorama/layout/citadel_hud_top_bar_team.xml",
    "panorama/layout/citadel_hud_top_bar_chat.xml",
    "panorama/layout/citadel_hud_hero_shop.xml",
    "panorama/layout/hud_paused.xml",
    "panorama/layout/hud.xml",
    "panorama/styles/citadel_hud_top_bar.css",
    "panorama/styles/hud.css",
    "panorama/styles/hud_paused.css",
    "panorama/styles/hero_testing_menu.css",
    "panorama/styles/hud_damage_report.css",
    "panorama/scripts/citadel_hud_top_bar.js",
    "panorama/scripts/hud.js",
    "panorama/scripts/recent_purchases.js",
]


def fetch(rev, path):
    out = os.path.join(HERE, rev, path)
    if os.path.exists(out):
        return out
    os.makedirs(os.path.dirname(out), exist_ok=True)
    url = RAW.format(rev=rev, path=path)
    try:
        with urllib.request.urlopen(url) as r:
            data = r.read()
    except urllib.error.HTTPError as e:
        print(f"{rev} {path}: HTTP {e.code}")
        return None
    with open(out, "wb") as f:
        f.write(data)
    print(f"{rev} {path}: {len(data)} bytes")
    return out


if __name__ == "__main__":
    revs = sys.argv[1:] or ["master", "24ebfd591c"]
    for rev in revs:
        for path in FILES:
            fetch(rev, path)
