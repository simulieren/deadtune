(function () {
    "use strict";
    var CONFIG = DT_MAP;
    var context = $.GetContextPanel();
    var map = null;
    var background = null;
    var topBar = null;
    var apples = null;
    var tunnels = null;
    var tunnelHeroes = {};
    var hero = { stable: false, candidate: "", samples: 0 };

    function valid(p) {
        try { return !!p && (!p.IsValid || p.IsValid()); } catch (e) { return false; }
    }
    function parent(p) {
        try { return p.GetParent ? p.GetParent() : null; } catch (e) { return null; }
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
    function hasClass(p, cls) {
        try { return !!(valid(p) && p.BHasClass && p.BHasClass(cls)); } catch (e) { return false; }
    }
    function setClass(p, cls, on) {
        try { if (valid(p)) { p.SetHasClass(cls, !!on); } } catch (e) {}
    }
    function upper(value) {
        return String(value || "").replace(/\s+/g, " ").replace(/^\s+|\s+$/g, "").toUpperCase();
    }

    (function heroNames() {
        var i;
        for (i = 0; i < CONFIG.heroes.length; i++) { tunnelHeroes[upper(CONFIG.heroes[i])] = true; }
        if (typeof $.Localize !== "function") { return; }
        for (i = 0; i < CONFIG.tokens.length; i++) {
            try {
                var name = upper($.Localize(CONFIG.tokens[i]));
                if (name && name !== upper(CONFIG.tokens[i])) { tunnelHeroes[name] = true; }
            } catch (e) {}
        }
    })();

    function root() {
        var p = context;
        for (var i = 0; i < 64 && valid(parent(p)); i++) { p = parent(p); }
        return p;
    }

    // The top bar's local portrait names the hero in the game's language; two equal
    // readings in a row are needed to show entrances, any other reading hides them at once.
    function canUseTunnels() {
        if (!valid(topBar)) { topBar = find(root(), "TopBar"); }
        var name = "";
        var local = byClass(topBar, "LocalPlayer");
        if (local.length === 1) {
            var labels = byClass(find(local[0], "PlayerNameNWContainer"), "HeroName");
            if (labels.length === 1) {
                try { name = upper(labels[0].text); } catch (e) { name = ""; }
            }
        }
        var eligible = name !== "" && tunnelHeroes[name] === true;
        if (!eligible) {
            hero.stable = false;
            hero.candidate = "";
            hero.samples = 0;
        } else if (name === hero.candidate) {
            hero.samples += 1;
        } else {
            hero.stable = false;
            hero.candidate = name;
            hero.samples = 1;
        }
        if (hero.samples >= 2) { hero.stable = true; }
        return hero.stable;
    }

    function inTunnels() {
        var p = map;
        for (var i = 0; i < 32 && valid(p); i++) {
            if (hasClass(p, "in_tunnels")) { return true; }
            p = parent(p);
        }
        return false;
    }

    function offsetWithin(p, ancestor) {
        var x = 0;
        var y = 0;
        for (var i = 0; i < 32 && valid(p) && p !== ancestor; i++) {
            var px = p.actualxoffset;
            var py = p.actualyoffset;
            if (typeof px !== "number" || typeof py !== "number" || !isFinite(px) || !isFinite(py)) { return null; }
            x += px;
            y += py;
            p = parent(p);
        }
        return p === ancestor ? { x: x, y: y } : null;
    }

    // Where the local hero's marker sits on the map picture, as fractions of its size.
    function heroOnMap() {
        var found = byClass(map, "localplayer");
        var marker = null;
        for (var i = 0; i < found.length; i++) {
            if (!hasClass(found[i], "player") || !hasClass(found[i], "friend")) { continue; }
            if (marker) { return null; }
            marker = found[i];
        }
        if (!valid(marker) || marker.visible === false || background.visible === false) { return null; }
        var m = offsetWithin(marker, map);
        var b = offsetWithin(background, map);
        var mw = marker.actuallayoutwidth;
        var mh = marker.actuallayoutheight;
        var bw = background.actuallayoutwidth;
        var bh = background.actuallayoutheight;
        if (!m || !b || !(mw > 0) || !(mh > 0) || !(bw > 0) || !(bh > 0)) { return null; }
        var u = (m.x + mw / 2 - b.x) / bw;
        var v = (m.y + mh / 2 - b.y) / bh;
        return u >= 0 && u <= 1 && v >= 0 && v <= 1 ? { u: u, v: v } : null;
    }

    function layer(id, dotClass, points) {
        var holder = $.CreatePanel("Panel", background, id, { hittest: "false", hittestchildren: "false" });
        var dots = [];
        for (var i = 0; i < points.length; i++) {
            var dot = $.CreatePanel("Panel", holder, id + i, { hittest: "false" });
            dot.AddClass(dotClass);
            dot.style.position = (points[i][0] * 100) + "% " + (points[i][1] * 100) + "% 0px";
            dot.dtNear = false;
            dots.push(dot);
        }
        return { panel: holder, dots: dots, points: points };
    }

    function ensureLayers() {
        if (!valid(map)) { map = find(context, "MinimapBackgroundTest") ? context : find(context, "hud_minimap"); }
        if (!valid(background)) { background = find(map, "MinimapBackgroundTest"); }
        if (!valid(background)) { return false; }
        var made = false;
        if (CONFIG.apples && !(apples && valid(apples.panel))) {
            apples = layer("DtApples", "DtAppleDot", CONFIG.apples);
            made = true;
        }
        if (CONFIG.tunnels && !(tunnels && valid(tunnels.panel))) {
            tunnels = layer("DtTunnelEntrances", "DtTunnelDot", CONFIG.tunnels);
            made = true;
        }
        if (made) {
            $.Msg("DeadTune minimap: " + (apples ? apples.dots.length : 0) + " apple spots, " +
                (tunnels ? tunnels.dots.length : 0) + " tunnel entrances");
        }
        return true;
    }

    // Entrances appear within `show` of the hero and stay until past `hide`, so one at the
    // edge does not flicker.
    function updateEntrances(at) {
        for (var i = 0; i < tunnels.dots.length; i++) {
            var dot = tunnels.dots[i];
            var near = false;
            if (at) {
                var du = tunnels.points[i][0] - at.u;
                var dv = tunnels.points[i][1] - at.v;
                var r = dot.dtNear ? CONFIG.hide : CONFIG.show;
                near = du * du + dv * dv <= r * r;
            }
            if (near !== dot.dtNear) {
                dot.dtNear = near;
                setClass(dot, "DtNear", near);
            }
        }
    }

    function tick() {
        if (!valid(context)) { return; }
        try {
            if (ensureLayers() && tunnels) {
                var show = canUseTunnels() && !inTunnels();
                updateEntrances(show ? heroOnMap() : null);
            }
        } catch (e) {
            try { $.Warning("DeadTune minimap: " + e); } catch (e2) {}
        }
        $.Schedule(CONFIG.tunnels ? 0.2 : 2.0, tick);
    }

    $.Schedule(1.0, tick);
})();
