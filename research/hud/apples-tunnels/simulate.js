// Runs crates/dt-core/src/hud/assets/apples_tunnels.js against a mock Panorama tree and
// checks hero gating, the show/hide radius, tunnel view and the apple layer.
// Usage: node research/hud/apples-tunnels/simulate.js
"use strict";
const fs = require("fs");
const path = require("path");
const assert = require("assert");

const script = fs.readFileSync(
    path.join(__dirname, "../../../crates/dt-core/src/hud/assets/apples_tunnels.js"), "utf8");

class Panel {
    constructor(id, parent, classes = []) {
        this.id = id;
        this.children = [];
        this.classes = new Set(classes);
        this.style = {};
        this.visible = true;
        this.actualxoffset = 0;
        this.actualyoffset = 0;
        this.actuallayoutwidth = 10;
        this.actuallayoutheight = 10;
        this.parent = parent || null;
        if (parent) parent.children.push(this);
    }
    IsValid() { return true; }
    GetParent() { return this.parent; }
    BHasClass(c) { return this.classes.has(c); }
    AddClass(c) { this.classes.add(c); }
    SetHasClass(c, on) { if (on) this.classes.add(c); else this.classes.delete(c); }
    FindChildTraverse(id) {
        for (const c of this.children) {
            if (c.id === id) return c;
            const f = c.FindChildTraverse(id);
            if (f) return f;
        }
        return null;
    }
    FindChildrenWithClassTraverse(cls) {
        const out = [];
        const walk = (p) => p.children.forEach((c) => { if (c.classes.has(cls)) out.push(c); walk(c); });
        walk(this);
        return out;
    }
}

const hud = new Panel("Hud", null, ["dl_midtown"]);
const topBar = new Panel("TopBar", hud);
const local = new Panel("", topBar, ["LocalPlayer"]);
const nameBox = new Panel("PlayerNameNWContainer", local);
const heroLabel = new Panel("", nameBox, ["HeroName"]);
const minimap = new Panel("hud_minimap", hud);
const background = new Panel("MinimapBackgroundTest", minimap);
background.actuallayoutwidth = 1000;
background.actuallayoutheight = 1000;
const marker = new Panel("", minimap, ["map_button", "player", "friend", "localplayer"]);

let scheduled = [];
const messages = [];
global.$ = {
    GetContextPanel: () => minimap,
    CreatePanel: (type, parent, id) => new Panel(id, parent),
    Schedule: (_, fn) => scheduled.push(fn),
    Msg: (m) => messages.push(m),
    Warning: (m) => { throw new Error(m); },
    Localize: (t) => (t === "#hero_familiar" ? "Rem (fr)" : t),
};
function tick(n = 1) {
    for (let i = 0; i < n; i++) {
        const run = scheduled;
        scheduled = [];
        run.forEach((f) => f());
    }
}
function place(u, v) {
    marker.actualxoffset = u * 1000 - 5;
    marker.actualyoffset = v * 1000 - 5;
}
function near() {
    const layer = background.FindChildTraverse("DtTunnelEntrances");
    return layer.children.filter((d) => d.classes.has("DtNear")).map((d) => d.id);
}

global.DT_MAP = {
    apples: [[0.2, 0.2], [0.8, 0.8]],
    tunnels: [[0.5, 0.5], [0.62, 0.5]],
    show: 0.11, hide: 0.13,
    heroes: ["Rem", "Mo & Krill", "Rat King", "Calico"],
    tokens: ["#hero_familiar", "#hero_krill", "#hero_ratking", "#hero_nano"],
};
eval(script);
place(0.5, 0.5);

heroLabel.text = "Haze";
tick(3);
assert.deepStrictEqual(messages, ["DeadTune minimap: 2 apple spots, 2 tunnel entrances"]);
const apples = background.FindChildTraverse("DtApples");
assert.strictEqual(apples.children.length, 2);
assert.strictEqual(apples.children[1].style.position, "80% 80% 0px");
assert.ok(apples.children[0].classes.has("DtAppleDot"));
assert.deepStrictEqual(near(), [], "other heroes see no entrances");

heroLabel.text = " mo  &  krill ";
tick(1);
assert.deepStrictEqual(near(), [], "one reading is not enough");
tick(1);
assert.deepStrictEqual(near(), ["DtTunnelEntrances0"], "0.12 away is outside 0.11");

place(0.515, 0.5);
tick(1);
assert.deepStrictEqual(near(), ["DtTunnelEntrances0", "DtTunnelEntrances1"]);
place(0.495, 0.5);
tick(1);
assert.deepStrictEqual(near(), ["DtTunnelEntrances0", "DtTunnelEntrances1"], "0.125 stays shown under 0.13");
place(0.48, 0.5);
tick(1);
assert.deepStrictEqual(near(), ["DtTunnelEntrances0"], "0.14 hides");

hud.AddClass("in_tunnels");
tick(1);
assert.deepStrictEqual(near(), [], "tunnel view hides entrances");
hud.SetHasClass("in_tunnels", false);
tick(1);
assert.deepStrictEqual(near(), ["DtTunnelEntrances0"]);

heroLabel.text = "Rem (FR)";
tick(1);
assert.deepStrictEqual(near(), [], "a hero change hides at once");
tick(1);
assert.deepStrictEqual(near(), ["DtTunnelEntrances0"], "localised names count");

marker.visible = false;
tick(1);
assert.deepStrictEqual(near(), [], "no marker, no entrances");
assert.strictEqual(messages.length, 1);
console.log("apples_tunnels.js simulation: ok");
