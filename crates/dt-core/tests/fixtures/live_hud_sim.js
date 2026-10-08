// Runs the live HUD script and the real bridge page (docs/bridge/index.html) against
// stand-ins: Panorama panels that take inline styles, and a CitadelHTMLPanel that loads
// only https:// pages, runs the page in its own context, delivers every title twice and
// turns a fragment-only SetURL into a hashchange. The page's fetches go to the DeadTune
// server the test runs, with the Origin a browser would send.
// Usage: node live_hud_sim.js <scenario.json>; prints a JSON report.
"use strict";
const fs = require("fs");
const vm = require("vm");

const sc = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const t0 = Date.now();
const events = [];
function log(kind, text) { events.push({ at: Date.now() - t0, kind: kind, text: String(text) }); }

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
    Object.assign(p, extra || {});
    return p;
}
function add(parent, child) { child.parent = parent; parent.kids.push(child); return child; }

const store = new Map(Object.entries(sc.storage || {}));
const localStorage = {
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => { store.set(k, String(v)); },
    removeItem: (k) => { store.delete(k); },
};
const pageScript = sc.page.match(/<script>([\s\S]*?)<\/script>/)[1];

function split(url) {
    const at = url.indexOf("#");
    return at < 0 ? [url, ""] : [url.slice(0, at), url.slice(at)];
}

function webPanel(id) {
    const p = panel(id, { paneltype: "CitadelHTMLPanel", handlers: [], page: null, loads: 0 });
    p.SetURL = (url) => {
        log("seturl", id + " " + url);
        const [path, hash] = split(url);
        if (p.page && p.page.path === path && hash && !sc.reload_on_hash) {
            p.page.hashchange(hash);
            return;
        }
        if (p.page) { p.page.close(); }
        p.loads++;
        p.page = load(p, path, hash);
    };
    return p;
}

function fire(p, title) {
    for (let i = 0; i < 2; i++) {
        for (const h of p.handlers) { h(p, title); }
    }
}

function load(owner, path, hash) {
    if (!path.startsWith("https://") || sc.page_fails) {
        setTimeout(() => fire(owner, "about:blank"), 5);
        return null;
    }
    let closed = false;
    const timers = new Set();
    const listeners = {};
    let title = "";
    const doc = {};
    Object.defineProperty(doc, "title", {
        get: () => title,
        set: (v) => {
            title = String(v).replace(/\s+/g, " ").trim();
            if (closed) { return; }
            const t = title;
            const late = sc.delay_restore_ms && t.indexOf(" restore dt1 ") > 0;
            setTimeout(() => fire(owner, t), late ? sc.delay_restore_ms : 0);
        },
    });
    const g = {
        location: { href: path + hash, search: new URL(path).search, hash: hash },
        document: doc,
        localStorage: localStorage,
        navigator: {},
        URLSearchParams: URLSearchParams,
        setTimeout: (f, ms) => {
            if (closed) { return 0; }
            const h = setTimeout(() => { timers.delete(h); if (!closed) { f(); } }, ms);
            timers.add(h);
            return h;
        },
        fetch: (url, opts) => {
            if (closed) { return new Promise(() => {}); }
            log("fetch", url);
            const headers = Object.assign({}, (opts || {}).headers, { Origin: "https://simulieren.github.io" });
            return fetch(url, Object.assign({}, opts, { headers: headers }));
        },
        addEventListener: (name, fn) => { (listeners[name] = listeners[name] || []).push(fn); },
    };
    g.window = g;
    vm.createContext(g);
    vm.runInContext(pageScript, g);
    return {
        path: path,
        close: () => { closed = true; for (const h of timers) { clearTimeout(h); } },
        hashchange: (h) => { g.location.hash = h; (listeners.hashchange || []).forEach((f) => f({})); },
    };
}

const root = panel("Hud");
const ctx = add(root, panel("DtLiveScriptHost"));
const styled = {};
for (const id of sc.panels) { styled[id] = add(root, panel(id)); }
const webPanels = {};

global.$ = {
    GetContextPanel: () => ctx,
    Schedule: (secs, fn) => { setTimeout(fn, secs * 1000); },
    Msg: (text) => log("msg", text),
    Warning: (text) => log("warn", text),
    DispatchEvent: (name, line) => log("cmd", name + " " + line),
    CreatePanel: (type, parent, id) => {
        if (type !== "CitadelHTMLPanel" || sc.no_panel) { return null; }
        const p = add(parent, webPanel(id));
        webPanels[id] = p;
        return p;
    },
    RegisterEventHandler: (name, p, fn) => { if (name === "HTMLTitle") { p.handlers.push(fn); } },
};

new Function(sc.script)();
setTimeout(() => {
    const styles = {};
    for (const id of sc.panels) { styles[id] = styled[id].style; }
    const loads = {};
    for (const id of Object.keys(webPanels)) { loads[id] = webPanels[id].loads; }
    process.stdout.write(JSON.stringify({ events: events, styles: styles, storage: Object.fromEntries(store), loads: loads }));
    process.exit(0);
}, sc.run_ms);
