(function () {
    "use strict";
    function say(text) { try { $.Msg("DEADTUNE_LIVE " + text); } catch (e) {} }
    function warn(text) { try { $.Warning("DEADTUNE_LIVE " + text); } catch (e) {} }
    say("loaded " + (typeof DT_LIVE === "undefined" ? "noconfig" : DT_LIVE.base));
    var CONFIG = DT_LIVE;
    var TOKEN = /#[\w-]+|\.[\w-]+|:not\(\.[\w-]+\)|[A-Za-z_][\w-]*/g;
    var ctx = $.GetContextPanel();
    var rules = [];
    var classed = false;
    var styled = [];
    var beats = 0;
    var restyles = 0;
    var titles = 0;

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
        restyles++;
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

    function apply(payload, full) {
        var incoming = [];
        var records = payload === "" ? [] : payload.split("~");
        for (var r = 0; r < records.length; r++) {
            var rule = compile(records[r]);
            if (rule) { incoming.push(rule); }
        }
        if (full) {
            rules = incoming;
        } else {
            for (var j = 0; j < incoming.length; j++) {
                var at = -1;
                for (var k = 0; k < rules.length; k++) { if (rules[k].key === incoming[j].key) { at = k; break; } }
                if (at < 0) { rules.push(incoming[j]); } else { rules[at] = incoming[j]; }
            }
        }
        classed = false;
        for (var c = 0; c < rules.length; c++) {
            var chain = rules[c].chain;
            for (var d = 0; d < chain.length; d++) { if (chain[d].cls.length || chain[d].not.length) { classed = true; } }
        }
        try { restyle(); } catch (e) { warn("error restyle " + e); }
    }

    // The web channel. The game's web panel loads only HTTPS pages, so it opens DeadTune's
    // page on GitHub Pages, which waits on DeadTune at 127.0.0.1 for messages and hands
    // them over as titles. Requests go back through the URL's fragment, which changes no
    // page. The panel stays tiny, nearly transparent and visible, so the page's timers
    // run, and it is never deleted. DeadTune wakes the page while someone edits the HUD
    // and puts it to sleep otherwise; asleep, the page only holds one open request.
    var web = null;
    var url = null;
    var ready = false;
    var awake = false;
    var heard = Date.now();
    var opened = 0;
    var reloads = 0;
    var asked = 0;
    var applied = null;
    var liveSeen = false;
    var held = null;
    var recent = [];
    var reported = {};

    function report(text) {
        if (reported[text]) { return; }
        reported[text] = true;
        say("web " + text);
    }
    function ask(verb, arg) {
        if (!web || !url) { return; }
        asked++;
        try { web.SetURL(url + "#" + asked + "." + verb + (arg === undefined ? "" : "." + arg)); } catch (e) { report("seturl error " + e); }
    }
    function open() {
        opened++;
        ready = false;
        awake = false;
        heard = Date.now();
        url = CONFIG.page + "?v=" + CONFIG.protocol.split(":v")[1] + "&port=" + CONFIG.port + "&base=" + CONFIG.base + (opened > 1 ? "&r=" + opened : "");
        try { web.SetURL(url); } catch (e) { report("seturl error " + e); }
    }
    // A page that never says ready is loaded again a few times, then left blank so an old
    // copy can't keep running.
    function watchdog() {
        if (ready) { return; }
        if (opened >= CONFIG.retries) {
            report("gave up");
            try { web.SetURL("about:blank"); } catch (e) {}
            return;
        }
        report("retry " + opened);
        open();
        $.Schedule(CONFIG.retry, watchdog);
    }

    function missing(msg) {
        var out = [];
        for (var i = 1; i <= msg.n; i++) { if (msg.parts[i] === undefined) { out.push(i); } }
        return out;
    }
    // A title lost at a low frame rate would lose the whole message; the page shows the
    // missing chunks again.
    function needLater(key, tries) {
        $.Schedule(CONFIG.need, function () {
            if (!held || held.key !== key || held.restore || tries <= 0) { return; }
            ask("need", held.seq + "." + missing(held).join(","));
            needLater(key, tries - 1);
        });
    }

    // One chunk of a message: `dt1 <seq> <i>/<n> <base> full|patch <payload part>`. A
    // restored message, the page's copy of the last one applied, counts only while no
    // message from DeadTune has started to arrive, so it never overwrites a newer edit.
    function chunk(body, restore) {
        var w = body.split(" ");
        if (w.length < 5 || w[0] !== "dt1") { return; }
        var seq = parseInt(w[1], 10);
        var of = w[2].split("/");
        var i = parseInt(of[0], 10), n = parseInt(of[1], 10);
        if (isNaN(seq) || !(n >= 1 && i >= 1 && i <= n)) { return; }
        if (restore) {
            if (liveSeen) { return; }
        } else {
            liveSeen = true;
            if (seq === applied) { return; }
        }
        var key = (restore ? "r" : "l") + seq;
        if (!held || held.key !== key) {
            held = { key: key, seq: seq, n: n, parts: {}, count: 0, base: w[3], full: w[4] === "full", restore: restore };
            if (n > 1) { needLater(key, 3); }
        }
        if (held.parts[i] !== undefined) { return; }
        held.parts[i] = w.length > 5 ? w.slice(5).join(" ") : "";
        held.count++;
        if (held.count < held.n) { return; }
        var msg = held;
        held = null;
        if (msg.base !== CONFIG.base) {
            if (!msg.restore) { say(msg.seq + " wrongbase " + CONFIG.base); }
            return;
        }
        var text = "";
        for (var k = 1; k <= msg.n; k++) { text += msg.parts[k]; }
        apply(text, msg.full);
        if (msg.restore) { report("restored " + msg.seq); return; }
        applied = msg.seq;
        say(msg.seq + " ok " + CONFIG.base);
        ask("ack", msg.seq);
    }

    // Titles can arrive twice; every one the page sets is distinct (a counter follows the
    // protocol word), so a repeat is dropped.
    function onTitle(panel, title) {
        title = String(title);
        if (title.indexOf("DTLIVE:") !== 0 || recent.indexOf(title) >= 0) { return; }
        recent.push(title);
        if (recent.length > 64) { recent.shift(); }
        titles++;
        heard = Date.now();
        if (title.indexOf(CONFIG.protocol + " ") !== 0) {
            report("old page " + title.split(" ")[0]);
            if (opened < CONFIG.retries) { open(); }
            return;
        }
        var rest = title.slice(CONFIG.protocol.length + 1);
        var body = rest.slice(rest.indexOf(" ") + 1);
        if (body.indexOf("dt1 ") === 0) { chunk(body, false); return; }
        if (body.indexOf("restore dt1 ") === 0) { chunk(body.slice(8), true); return; }
        if (body === "wake") { awake = true; return; }
        if (body === "sleep") { awake = false; return; }
        if (body === "beat") { return; }
        if (body.indexOf("ready") === 0) {
            report(body);
            if (!ready) { ready = true; ask("restore"); }
            return;
        }
        report(body);
    }

    function start() {
        web = $.CreatePanel("CitadelHTMLPanel", ctx, "DtLiveWeb");
        if (!web) { report("nopanel"); return; }
        try { web.hittest = false; } catch (e) {}
        try { web.style.width = "2px"; web.style.height = "2px"; web.style.opacity = "0.01"; } catch (e) {}
        $.RegisterEventHandler("HTMLTitle", web, onTitle);
        open();
        $.Schedule(CONFIG.retry, watchdog);
    }

    // Rules keyed on a class follow panels as they come and go; nothing else runs on a
    // timer. An awake page that went quiet is loaded again.
    var beatFailed = false;
    function beat() {
        beats++;
        try {
            if (classed && (rules.length || styled.length)) { restyle(); }
        } catch (e) {
            if (!beatFailed) { beatFailed = true; warn("error beat " + e); }
        }
        if (awake && ready && Date.now() - heard > CONFIG.quiet * 1000) {
            reloads++;
            say("web reload " + reloads);
            open();
            $.Schedule(CONFIG.retry, watchdog);
        }
        $.Schedule(awake ? CONFIG.beat : CONFIG.sleepBeat, beat);
    }

    try { start(); } catch (e) { report("error " + e); }
    say("hello " + CONFIG.base + " web=" + (web ? "panel" : "nopanel"));
    $.Schedule(10, function () { say("alive 10s beats=" + beats + " restyles=" + restyles + " titles=" + titles); });
    $.Schedule(CONFIG.beat, beat);
})();
