(function () {
    "use strict";
    var CONFIG = DT_LIVE;
    var TOKEN = /#[\w-]+|\.[\w-]+|:not\(\.[\w-]+\)|[A-Za-z_][\w-]*/g;
    var CHUNK = /^dt1 (\d+) (\d+)\/(\d+) (\S+) (full|patch)(?: (\S*))?$/;
    var ctx = $.GetContextPanel();
    var held = null;
    var applied = null;
    var lastWrong = null;
    var rules = [];
    var styled = [];
    var clock = 0;
    var nextHello = 0;
    var nextExec = 0;
    var hotUntil = -1;

    function valid(p) {
        try { return !!p && (!p.IsValid || p.IsValid()); } catch (e) { return false; }
    }
    function parent(p) {
        try { return valid(p) && p.GetParent ? p.GetParent() : null; } catch (e) { return null; }
    }
    function root() {
        var p = ctx;
        for (var i = 0; i < 64; i++) {
            var up = parent(p);
            if (!valid(up)) { break; }
            p = up;
        }
        return p;
    }
    function find(p, id) {
        try { return valid(p) && p.FindChildTraverse ? p.FindChildTraverse(id) : null; } catch (e) { return null; }
    }
    function byClass(p, cls) {
        try {
            var all = valid(p) && p.FindChildrenWithClassTraverse ? p.FindChildrenWithClassTraverse(cls) : null;
            return all && all.length ? all : [];
        } catch (e) { return []; }
    }
    function children(p) {
        try { return valid(p) && p.Children ? p.Children() : []; } catch (e) { return []; }
    }
    function hasClass(p, cls) {
        try { return !!(valid(p) && p.BHasClass && p.BHasClass(cls)); } catch (e) { return false; }
    }
    function cmd(c) {
        try { $.DispatchEvent("CitadelConCommand", c); } catch (e) {}
    }
    function echo(text) { cmd("echo DEADTUNE_LIVE " + text); }
    function readSlot(name) {
        try {
            var v = GameInterfaceAPI.GetSettingString(name);
            return v === undefined || v === null ? null : String(v);
        } catch (e) { return null; }
    }
    function unescape(s) {
        try { return decodeURIComponent(s); } catch (e) { return null; }
    }
    function camel(prop) {
        return prop.replace(/-([a-z0-9])/g, function (m, c) { return c.toUpperCase(); });
    }

    function compound(text) {
        var c = { id: null, tag: null, cls: [], not: [] };
        var tokens = text.match(TOKEN) || [];
        for (var i = 0; i < tokens.length; i++) {
            var t = tokens[i];
            if (t.charAt(0) === "#") { c.id = t.slice(1); }
            else if (t.charAt(0) === ".") { c.cls.push(t.slice(1)); }
            else if (t.charAt(0) === ":") { c.not.push(t.slice(6, -1)); }
            else { c.tag = t; }
        }
        return c;
    }
    function compile(record) {
        var f = record.split("^");
        if (f.length !== 3) { return null; }
        var sel = unescape(f[0]), prop = unescape(f[1]), value = unescape(f[2]);
        if (sel === null || prop === null || value === null) { return null; }
        return { key: sel + "^" + prop, chain: sel.split(/\s+/).map(compound), prop: camel(prop), value: value };
    }

    function matchesCompound(p, c) {
        if (!valid(p)) { return false; }
        try {
            if (c.id !== null && p.id !== c.id) { return false; }
            if (c.tag !== null && p.paneltype !== c.tag) { return false; }
        } catch (e) { return false; }
        for (var i = 0; i < c.cls.length; i++) { if (!hasClass(p, c.cls[i])) { return false; } }
        for (var j = 0; j < c.not.length; j++) { if (hasClass(p, c.not[j])) { return false; } }
        return true;
    }
    function walk(p, out) {
        var kids = children(p);
        for (var i = 0; i < kids.length; i++) { out.push(kids[i]); walk(kids[i], out); }
        return out;
    }
    // Ids repeat across list items (every minimap marker has a #BackgroundImage), so an id
    // is looked up inside each match of the compound before it.
    function candidates(top, chain) {
        var last = chain[chain.length - 1];
        if (last.cls.length) { return byClass(top, last.cls[0]); }
        if (last.id === null) { return walk(top, []); }
        var scopes = chain.length > 1 ? candidates(top, chain.slice(0, -1)) : [top];
        var out = [];
        for (var i = 0; i < scopes.length; i++) {
            var p = find(scopes[i], last.id);
            if (p && out.indexOf(p) < 0) { out.push(p); }
        }
        return out;
    }
    function ancestorsMatch(p, chain) {
        var k = chain.length - 2;
        for (var q = parent(p); k >= 0 && valid(q); q = parent(q)) {
            if (matchesCompound(q, chain[k])) { k--; }
        }
        return k < 0;
    }

    function restyle() {
        var top = root();
        var want = [];
        for (var r = 0; r < rules.length; r++) {
            var rule = rules[r];
            var found = candidates(top, rule.chain);
            for (var i = 0; i < found.length; i++) {
                var p = found[i];
                if (!matchesCompound(p, rule.chain[rule.chain.length - 1]) || !ancestorsMatch(p, rule.chain)) { continue; }
                var hit = null;
                for (var w = 0; w < want.length; w++) {
                    if (want[w].panel === p && want[w].prop === rule.prop) { hit = want[w]; break; }
                }
                if (hit) { hit.value = rule.value; } else { want.push({ panel: p, prop: rule.prop, value: rule.value }); }
            }
        }
        for (var s = 0; s < styled.length; s++) {
            var old = styled[s];
            var kept = false;
            for (var k = 0; k < want.length; k++) {
                if (want[k].panel === old.panel && want[k].prop === old.prop) { kept = true; break; }
            }
            if (!kept && valid(old.panel)) {
                try { old.panel.style[old.prop] = null; } catch (e) {}
            }
        }
        for (var n = 0; n < want.length; n++) {
            var set = want[n];
            var prev = null;
            for (var o = 0; o < styled.length; o++) {
                if (styled[o].panel === set.panel && styled[o].prop === set.prop) { prev = styled[o]; break; }
            }
            if (!prev || prev.value !== set.value) {
                try { set.panel.style[set.prop] = set.value; } catch (e) {}
            }
        }
        styled = want;
    }

    function apply(msg) {
        var payload = "";
        for (var i = 1; i <= msg.n; i++) { payload += msg.parts[i]; }
        var incoming = [];
        var records = payload === "" ? [] : payload.split("~");
        for (var r = 0; r < records.length; r++) {
            var rule = compile(records[r]);
            if (rule) { incoming.push(rule); }
        }
        if (msg.kind === "full") { rules = incoming; return; }
        for (var j = 0; j < incoming.length; j++) {
            var at = -1;
            for (var k = 0; k < rules.length; k++) { if (rules[k].key === incoming[j].key) { at = k; break; } }
            if (at < 0) { rules.push(incoming[j]); } else { rules[at] = incoming[j]; }
        }
    }

    function receive() {
        var fresh = false;
        for (var s = 0; s < CONFIG.slots.length; s++) {
            var m = CHUNK.exec(readSlot(CONFIG.slots[s]) || "");
            if (!m) { continue; }
            var seq = Number(m[1]), i = Number(m[2]), n = Number(m[3]);
            if (m[4] !== CONFIG.base) {
                if (lastWrong !== seq) { lastWrong = seq; echo(seq + " wrongbase " + CONFIG.base); }
                continue;
            }
            if (seq === applied || i < 1 || i > n) { continue; }
            if (!held || held.seq !== seq) { held = { seq: seq, n: n, kind: m[5], parts: {}, count: 0 }; }
            if (held.parts[i] === undefined) { held.parts[i] = m[6] || ""; held.count++; fresh = true; }
        }
        if (held && held.count === held.n) {
            apply(held);
            applied = held.seq;
            held = null;
            echo(applied + " ok " + CONFIG.base);
        } else if (held && fresh) {
            echo(held.seq + " got " + held.count + " " + CONFIG.base);
        }
        return fresh;
    }

    function hello() {
        var read = readSlot("tv_title");
        echo("hello " + CONFIG.base + (read === null ? "" : " tv_title=" + encodeURIComponent(read.slice(0, 24))));
    }

    // The console may log every exec, so the cfg is read once a second until DeadTune
    // sends something and four times a second while it does.
    function execCfg() {
        if (!CONFIG.cfg || clock < nextExec) { return; }
        cmd("exec " + CONFIG.cfg);
        nextExec = clock + (clock < hotUntil ? CONFIG.poll : CONFIG.idle);
    }

    function poll() {
        clock += CONFIG.poll;
        execCfg();
        if (receive()) { hotUntil = clock + CONFIG.hot; nextExec = Math.min(nextExec, clock + CONFIG.poll); }
        if (rules.length || styled.length) { restyle(); }
        if (clock >= nextHello) { nextHello = clock + CONFIG.hello; hello(); }
        $.Schedule(CONFIG.poll, poll);
    }

    nextHello = CONFIG.hello;
    hello();
    $.Schedule(CONFIG.poll, poll);
})();
