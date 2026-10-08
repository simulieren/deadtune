(function () {
    "use strict";
    function say(text) { try { $.Msg("DEADTUNE_LIVE " + text); } catch (e) {} }
    function warn(text) { try { $.Warning("DEADTUNE_LIVE " + text); } catch (e) {} }
    say("loaded " + (typeof DT_LIVE === "undefined" ? "noconfig" : DT_LIVE.base));
    var CONFIG = DT_LIVE;
    var TOKEN = /#[\w-]+|\.[\w-]+|:not\(\.[\w-]+\)|[A-Za-z_][\w-]*/g;
    var ctx = $.GetContextPanel();
    var held = null;
    var applied = null;
    var lastCtl = null;
    var rules = [];
    var styled = [];
    var clock = 0;
    var pullUntil = -1;
    var nextPull = 0;
    var greeted = false;

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
    function unescape(s) {
        try { return decodeURIComponent(s); } catch (e) { return null; }
    }
    function camel(prop) {
        return prop.replace(/-([a-z0-9])/g, function (m, c) { return c.toUpperCase(); });
    }

    // The stock slider control holds a text box ("Value") and a 0..1 slider ("Slider").
    // Every slot holds a whole number, so separators of any locale are dropped with the rest.
    function number(text) {
        var digits = String(text).replace(/[^0-9]/g, "");
        return digits === "" ? null : parseInt(digits, 10);
    }
    function slotText(row) {
        var text = find(row, "Value");
        try { return text ? String(text.text) : null; } catch (e) { return null; }
    }
    function slotSlider(row) {
        var slider = find(row, "Slider");
        try { return slider ? Number(slider.value) : null; } catch (e) { return null; }
    }
    function slotValue(id, max) {
        var row = find(root(), id);
        if (!row) { return null; }
        var v = number(slotText(row) || "");
        if (v === null) {
            var s = slotSlider(row);
            v = s === null || isNaN(s) ? null : s * max;
        }
        return v === null ? null : Math.round(v);
    }
    function readCtl() { return slotValue(CONFIG.ctl, CONFIG.ctlMax); }
    function readData() {
        var out = [];
        for (var k = 0; k < CONFIG.data.length; k++) {
            var v = slotValue(CONFIG.data[k], CONFIG.max);
            out.push(v === null ? 0 : v);
        }
        return out;
    }

    function expand(bytes) {
        var s = "";
        for (var i = 0; i < bytes.length; i++) {
            var b = bytes[i];
            s += b >= 128 ? (CONFIG.dict[b - 128] || "") : String.fromCharCode(b);
        }
        return s;
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
        var words = [];
        for (var i = 1; i <= msg.n; i++) { words = words.concat(msg.parts[i]); }
        var bytes = [];
        for (var w = 0; w < words.length; w++) { bytes.push(words[w] >> 8); bytes.push(words[w] & 255); }
        var payload = expand(bytes.slice(0, msg.len));
        var incoming = [];
        var records = payload === "" ? [] : payload.split("~");
        for (var r = 0; r < records.length; r++) {
            var rule = compile(records[r]);
            if (rule) { incoming.push(rule); }
        }
        if (msg.full) { rules = incoming; return; }
        for (var j = 0; j < incoming.length; j++) {
            var at = -1;
            for (var k = 0; k < rules.length; k++) { if (rules[k].key === incoming[j].key) { at = k; break; } }
            if (at < 0) { rules.push(incoming[j]); } else { rules[at] = incoming[j]; }
        }
    }

    // The control slot changes once per chunk: sequence in the high bits, chunk number in
    // the low ten (zero is no chunk). Chunk 1 opens with the chunk count, the pull flag and
    // the kind, the payload length and the base.
    // Chunk 0 under a new sequence asks for a hello, so a console line can check the read.
    function receive() {
        var ctl = readCtl();
        if (ctl === null || ctl === lastCtl) { return; }
        lastCtl = ctl;
        var seq = ctl >> 10, i = ctl & 1023;
        if (i === 0) {
            if (greeted) { hello(); }
            return;
        }
        if (seq === applied) { return; }
        var words = readData();
        if (!held || held.seq !== seq) { held = { seq: seq, n: 0, parts: {}, count: 0, pull: false, full: false, len: 0, base: null }; }
        if (held.parts[i] !== undefined) { return; }
        if (i === 1) {
            held.n = words[0] >> 2;
            held.pull = (words[0] & 2) === 2;
            held.full = (words[0] & 1) === 1;
            held.len = words[1];
            held.base = words[2] === CONFIG.baseWords[0] && words[3] === CONFIG.baseWords[1];
            held.parts[1] = words.slice(4);
        } else {
            held.parts[i] = words;
        }
        held.count++;
        if (held.base === false) {
            echo(seq + " wrongbase " + CONFIG.base);
            held = null;
            pullUntil = -1;
            return;
        }
        if (held.n > 0 && held.count === held.n) {
            apply(held);
            applied = seq;
            held = null;
            pullUntil = -1;
            echo(seq + " ok " + CONFIG.base);
            return;
        }
        echo(seq + " got " + held.count + " " + CONFIG.base);
        if (held.pull) {
            pullUntil = clock + CONFIG.pullTimeout;
            nextPull = clock + CONFIG.pull;
        }
    }

    // Only while a pulled message is coming in does the script run the cfg itself; every
    // exec prints a console line, so an idle HUD prints nothing.
    function pull() {
        if (pullUntil < 0 || clock < nextPull) { return; }
        if (clock > pullUntil) { pullUntil = -1; return; }
        cmd("exec " + CONFIG.cfg);
        nextPull = clock + CONFIG.pull;
    }

    function probes() {
        var top = root();
        var out = [];
        var ctlRow = find(top, CONFIG.ctl);
        out.push("ctl=" + readCtl());
        out.push("raw=" + (ctlRow ? slotText(ctlRow) + "/" + slotSlider(ctlRow) : "none"));
        out.push("col=" + slotValue(CONFIG.probe, CONFIG.ctlMax));
        out.push("d=" + readData().join(","));
        var found = 0;
        for (var k = 0; k < CONFIG.data.length; k++) { if (find(top, CONFIG.data[k])) { found++; } }
        out.push("n=" + found);
        var gi = "0";
        try {
            if (typeof GameInterfaceAPI !== "undefined") {
                gi = "1" + (GameInterfaceAPI.GetSettingString ? "s" : "") + (GameInterfaceAPI.GetSettingValue ? "v" : "") + (GameInterfaceAPI.SetSettingValue ? "w" : "");
            }
        } catch (e) { gi = "e"; }
        out.push("api=" + (gi === "0" ? "missing" : gi));
        var has = function (f) { try { return typeof f === "function" ? "1" : "0"; } catch (e) { return "e"; } };
        out.push("kv=" + has($.LoadKeyValues));
        out.push("kvf=" + has($.LoadKeyValuesFile));
        out.push("ld=" + has(ctx.BLoadLayoutFromString));
        out.push("cp=" + has($.CreatePanel));
        var html = "0";
        try {
            var p = $.CreatePanel("CitadelHTMLPanel", ctx, "DtLiveHtmlProbe");
            html = p ? (p.SetURL ? "1" : "p") : "0";
            if (p && p.DeleteAsync) { p.DeleteAsync(0); }
        } catch (e) { html = "e"; }
        out.push("html=" + html);
        return out.join(" ");
    }

    var pollFailed = false;
    function poll() {
        clock += CONFIG.poll;
        try {
            receive();
            pull();
            if (rules.length || styled.length) { restyle(); }
        } catch (e) {
            if (!pollFailed) { pollFailed = true; warn("error poll " + e); }
        }
        $.Schedule(CONFIG.poll, poll);
    }

    function hello() {
        var extra;
        try { extra = probes(); } catch (e) { extra = "probe_error=" + encodeURIComponent(String(e)).slice(0, 80); }
        var line = "hello " + CONFIG.base + " " + extra;
        say(line);
        echo(line);
    }

    // Can the web panel load a page from this PC? DeadTune serves one on 127.0.0.1; the
    // page answers through its title, which arrives as an HTMLTitle event.
    function webProbe() {
        var urls = [
            ["ip", "http://127.0.0.1:" + CONFIG.webPort + "/probe?k=ip"],
            ["host", "http://localhost:" + CONFIG.webPort + "/probe?k=host"],
            ["data", "data:text/html,<title>DTLIVE data ok</title>"]
        ];
        for (var i = 0; i < urls.length; i++) {
            try {
                var kind = urls[i][0];
                var p = $.CreatePanel("CitadelHTMLPanel", ctx, "DtLiveWeb_" + kind);
                if (!p) { say("web " + kind + " nopanel"); continue; }
                p.hittest = false;
                if (p.style) { p.style.width = "2px"; p.style.height = "2px"; p.style.opacity = "0.01"; }
                $.RegisterEventHandler("HTMLTitle", p, (function (k) {
                    return function (panel, title) { say("web " + k + " title=" + title); };
                })(kind));
                p.SetURL(urls[i][1]);
                say("web " + kind + " requested");
            } catch (e) { say("web " + urls[i][0] + " error " + e); }
        }
    }

    // The sliders read their ConVars as they come up, so the hello waits a moment.
    $.Schedule(2.0, function () { try { webProbe(); } catch (e) { warn("error web " + e); } });
    $.Schedule(1.0, function () { greeted = true; hello(); });
    $.Schedule(CONFIG.poll, poll);
})();
