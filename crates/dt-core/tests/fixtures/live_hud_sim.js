// Runs the live HUD script against a stand-in for Panorama: hidden sliders that show their
// ConVar's value, panels that take inline styles, and a console that records every command.
// Usage: node live_hud_sim.js <scenario.json>; prints a JSON report.
"use strict";
const fs = require("fs");

const sc = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const convars = {};
const timers = [];
let now = 0;
const cmds = [];
let cfg = [];

function panel(id, extra) {
    const p = {
        id: id,
        paneltype: "Panel",
        style: {},
        kids: [],
        parent: null,
        IsValid: () => true,
        GetParent: () => p.parent,
        Children: () => p.kids,
        BHasClass: () => false,
        FindChildTraverse: (want) => {
            for (const k of p.kids) {
                if (k.id === want) { return k; }
                const deep = k.FindChildTraverse(want);
                if (deep) { return deep; }
            }
            return null;
        },
        FindChildrenWithClassTraverse: () => [],
    };
    Object.defineProperties(p, Object.getOwnPropertyDescriptors(extra || {}));
    return p;
}
function add(parent, child) { child.parent = parent; parent.kids.push(child); return child; }

const root = panel("Hud");
const ctx = add(root, panel("DtLiveScriptHost"));
const slots = add(root, panel("DtLive"));
function slider(id, convar) {
    const row = add(slots, panel(id, { paneltype: "CitadelSettingsSlider" }));
    add(row, panel("Value", { get text() { return sc.locale_text(convars[convar]); } }));
    add(row, panel("Slider", { get value() { return 0; } }));
}
sc.locale_text = (v) => v === undefined ? "" : Number(v).toLocaleString("de-DE");
slider(sc.ctl.id, sc.ctl.convar);
for (const d of sc.data) { slider(d.id, d.convar); }
slider(sc.probe_id, sc.ctl.convar);
const styled = {};
for (const id of sc.panels) { styled[id] = add(root, panel(id)); }

function set(lines) { for (const [name, value] of lines) { convars[name] = value; } }
set(sc.boot);

global.$ = {
    GetContextPanel: () => ctx,
    Schedule: (secs, fn) => { timers.push({ at: now + secs, fn: fn }); },
    DispatchEvent: (name, line) => {
        if (name !== "CitadelConCommand") { return; }
        cmds.push({ at: now, line: line });
        if (line === sc.exec) { set(cfg); }
    },
    CreatePanel: () => null,
};

function run(until) {
    for (;;) {
        timers.sort((a, b) => a.at - b.at);
        if (!timers.length || timers[0].at > until) { break; }
        const t = timers.shift();
        now = t.at;
        t.fn();
    }
    now = until;
}
function since(t) { return cmds.filter((c) => c.at >= t).map((c) => c.line); }

new Function(sc.script)();
const report = {};
run(5);
report.start = since(0);
run(65);
report.idle = since(5);

let t = now;
set(sc.chunks[0]);
let next = 1;
const deadline = now + 30;
while (now < deadline) {
    run(now + 0.05);
    const got = cmds.filter((c) => c.at >= t).map((c) => c.line);
    const last = got.filter((l) => l.indexOf(" got ") >= 0).length;
    if (last >= next && next < sc.chunks.length) {
        cfg = sc.chunks[next];
        next++;
    }
    if (got.some((l) => l.indexOf(" ok ") >= 0)) { break; }
}
run(now + 10);
report.message = since(t);
report.styles = {};
for (const id of sc.panels) { report.styles[id] = styled[id].style; }

t = now;
run(now + 60);
report.after = since(t);

t = now;
set(sc.ping);
run(now + 2);
report.ping = since(t);

process.stdout.write(JSON.stringify(report));
