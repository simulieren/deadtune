(function () {
    "use strict";
    var CONFIG = DT_TOP_BAR;
    var POWERUP_CYCLE = 300;
    var REJUV_PHASES = [413, 353, 293];
    var PURCHASE_SECONDS = 10;
    var PURCHASE_MAX = 3;
    var top = $.GetContextPanel();
    var ui = {};
    var rejuv = { claims: 0, phaseStart: 0, lastCharge: false };
    var purchases = { seen: {}, seeded: false, byHero: {} };
    var lastClock = -1;

    function valid(p) {
        try { return !!p && (!p.IsValid || p.IsValid()); } catch (e) { return false; }
    }
    function root() {
        var p = top;
        for (var i = 0; i < 64 && valid(p); i++) {
            var parent = null;
            try { parent = p.GetParent ? p.GetParent() : null; } catch (e) { parent = null; }
            if (!valid(parent)) { break; }
            p = parent;
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
    function hasClass(p, cls) {
        try { return !!(valid(p) && p.BHasClass && p.BHasClass(cls)); } catch (e) { return false; }
    }
    function setClass(p, cls, on) {
        try { if (valid(p)) { p.SetHasClass(cls, !!on); } } catch (e) {}
    }
    function text(p) {
        try { return valid(p) && p.text !== undefined && p.text !== null ? String(p.text) : ""; } catch (e) { return ""; }
    }
    function setText(p, value) {
        try { if (valid(p) && text(p) !== String(value)) { p.text = String(value); } } catch (e) {}
    }
    function panel(type, parent, id, cls) {
        var p = $.CreatePanel(type, parent, id);
        if (cls) { p.AddClass(cls); }
        return p;
    }
    function pad(n) { return (n < 10 ? "0" : "") + n; }
    function fmt(seconds) {
        var s = Math.max(0, Math.floor(seconds));
        return Math.floor(s / 60) + ":" + pad(s % 60);
    }
    function parseClock(value) {
        var m = String(value || "").match(/(\d+)\s*:\s*(\d{1,2})/);
        return m ? Number(m[1]) * 60 + Number(m[2]) : -1;
    }
    function parseNumber(value) {
        var m = String(value || "").replace(/,/g, "").trim().toLowerCase().match(/^([0-9]*\.?[0-9]+)\s*([km])?$/);
        if (!m) { return null; }
        var n = parseFloat(m[1]);
        if (m[2] === "k") { n *= 1000; }
        if (m[2] === "m") { n *= 1000000; }
        return isFinite(n) ? Math.round(n) : null;
    }
    function clockSeconds() {
        try {
            if (typeof Game !== "undefined" && Game) {
                if (typeof Game.GetGameTime === "function") { return Math.floor(Number(Game.GetGameTime())); }
                if (typeof Game.GetDOTATime === "function") { return Math.floor(Number(Game.GetDOTATime())); }
            }
            if (typeof GameUI !== "undefined" && GameUI && typeof GameUI.GetGameTime === "function") {
                return Math.floor(Number(GameUI.GetGameTime()));
            }
        } catch (e) {}
        return parseClock(text(find(top, "GameTime")));
    }
    function inHideout() {
        var hud = find(root(), "Hud");
        return hasClass(hud, "connectedToHideout") || hasClass(hud, "InHideout") || hasClass(top, "connectedToHideout");
    }

    function ensurePanels() {
        if (CONFIG.spawnTimers && !valid(ui.timers)) {
            ui.timers = panel("Panel", top, "DtSpawnTimers");
            ui.powerup = panel("Panel", ui.timers, "DtPowerup", "DtTimer");
            panel("Panel", ui.powerup, "DtPowerupIcon", "DtTimerIcon");
            ui.powerupLabel = panel("Label", ui.powerup, "DtPowerupLabel", "DtTimerLabel");
            ui.rejuv = panel("Panel", ui.timers, "DtRejuv", "DtTimer");
            panel("Panel", ui.rejuv, "DtRejuvIcon", "DtTimerIcon");
            ui.rejuvLabel = panel("Label", ui.rejuv, "DtRejuvLabel", "DtTimerLabel");
        }
        if (CONFIG.urnLead && !valid(ui.urn)) {
            var networth = byClass(top, "TeamNetworth")[0];
            if (valid(networth)) {
                ui.urn = panel("Panel", networth, "DtUrnLead");
                ui.urnLabel = panel("Label", ui.urn, "DtUrnLeadLabel");
            }
        }
    }

    function warn(p, remaining, ready) {
        setClass(p, "DtSoon", !ready && remaining > 0 && remaining <= 30);
        setClass(p, "DtNow", ready);
    }

    function teamHasCharge() {
        var charges = find(top, "RejuvenatorCharges");
        var sides = [find(charges, "RejuvenatorFriendly"), find(charges, "RejuvenatorEnemy")];
        for (var i = 0; i < sides.length; i++) {
            for (var n = 1; n <= 4; n++) {
                if (hasClass(sides[i], "RejuvCount_" + n)) { return true; }
            }
        }
        return false;
    }

    function rejuvText(now) {
        var timer = find(top, "RejuvenatorTimer");
        if (hasClass(timer, "midboss_cooldown")) {
            var own = text(find(timer, "MidbossTimerLabel"));
            if (parseClock(own) >= 0) { return { text: own, ready: false, remaining: parseClock(own) }; }
        }
        if (hasClass(timer, "midboss_available")) {
            return { text: "UP", ready: true, remaining: 0 };
        }
        var charge = teamHasCharge();
        if (charge && !rejuv.lastCharge) {
            rejuv.claims += 1;
            rejuv.phaseStart = now;
        }
        rejuv.lastCharge = charge;
        var phase = Math.min(rejuv.claims, REJUV_PHASES.length - 1);
        var elapsed = rejuv.claims === 0 ? now : now - rejuv.phaseStart;
        var remaining = REJUV_PHASES[phase] - elapsed;
        if (remaining <= 0) { return { text: "UP", ready: true, remaining: 0 }; }
        return { text: fmt(remaining), ready: false, remaining: remaining };
    }

    function updateTimers(now) {
        if (!CONFIG.spawnTimers || !valid(ui.timers)) { return; }
        var powerup = POWERUP_CYCLE - (now % POWERUP_CYCLE);
        setText(ui.powerupLabel, fmt(powerup));
        warn(ui.powerup, powerup, false);
        var r = rejuvText(now);
        setText(ui.rejuvLabel, r.text);
        warn(ui.rejuv, r.remaining, r.ready);
    }

    function teamScore(id) {
        var labels = byClass(find(top, id), "ScoreLabel");
        return labels.length ? parseNumber(text(labels[0])) : null;
    }

    function updateUrnLead(now) {
        if (!CONFIG.urnLead || !valid(ui.urn)) { return; }
        var friendly = teamScore("TeamScoreFriendly");
        var enemy = teamScore("TeamScoreEnemy");
        if (friendly === null || enemy === null || (friendly === 0 && enemy === 0)) {
            setText(ui.urnLabel, "--");
            setClass(ui.urn, "DtAhead", false);
            setClass(ui.urn, "DtBehind", false);
            return;
        }
        var pct = (friendly - enemy) / Math.max(friendly, enemy, 1) * 100;
        var threshold = now >= 900 ? 10 : 15;
        setText(ui.urnLabel, (pct >= 0 ? "+" : "") + pct.toFixed(0) + "%");
        setClass(ui.urn, "DtAhead", pct >= threshold);
        setClass(ui.urn, "DtBehind", pct <= -threshold);
    }

    function heroId(p) {
        try { var id = Number(p.heroid); return isFinite(id) && id > 0 ? id : 0; } catch (e) { return 0; }
    }

    function portraitFor(heroid) {
        if (!heroid) { return null; }
        var players = byClass(top, "PlayerDetailsContainer");
        for (var i = 0; i < players.length; i++) {
            if (heroId(find(players[i], "HeroBadge")) === heroid) {
                var player = players[i];
                try { player = player.GetParent() || player; } catch (e) {}
                return player;
            }
        }
        return null;
    }

    function purchaseKey(entry) {
        var name = text(byClass(entry, "recentModPurchaseName")[0]);
        var when = text(byClass(entry, "recentTimePurchased")[0]);
        var hero = heroId(find(entry, "RecentPurchaseHeroImage"));
        return name && when ? { key: name + "|" + when + "|" + hero, name: name, hero: hero } : null;
    }

    function showPurchase(info, entry) {
        var portrait = portraitFor(info.hero);
        if (!valid(portrait)) { return false; }
        var host = purchases.byHero[info.hero];
        if (!valid(host)) {
            host = panel("Panel", portrait, "", "DtPurchases");
            purchases.byHero[info.hero] = host;
        }
        var children = [];
        try { children = host.Children() || []; } catch (e) {}
        while (children.length >= PURCHASE_MAX) {
            try { children[0].DeleteAsync(0); } catch (e) {}
            children = children.slice(1);
        }
        var popup = panel("Panel", host, "", "DtPurchase");
        var classes = ["isTier1Purchase", "isTier2Purchase", "isTier3Purchase", "isTier4Purchase", "isWeaponPurchase", "isArmorPurchase", "isTechPurchase"];
        for (var i = 0; i < classes.length; i++) {
            if (hasClass(entry, classes[i])) { popup.AddClass(classes[i]); }
        }
        var label = panel("Label", popup, "", "DtPurchaseName");
        label.text = info.name;
        $.Schedule(PURCHASE_SECONDS, function () {
            setClass(popup, "DtFading", true);
            $.Schedule(0.35, function () { try { if (valid(popup)) { popup.DeleteAsync(0); } } catch (e) {} });
        });
        return true;
    }

    function updatePurchases() {
        if (!CONFIG.purchases) { return; }
        var container = find(root(), "RecentPurchasesContainer");
        var entries = byClass(container, "recentPurchase");
        for (var i = 0; i < entries.length; i++) {
            var info = purchaseKey(entries[i]);
            if (!info || purchases.seen[info.key]) { continue; }
            if (!purchases.seeded || showPurchase(info, entries[i])) { purchases.seen[info.key] = true; }
        }
        purchases.seeded = true;
    }

    function resetMatchState() {
        rejuv.claims = 0;
        rejuv.phaseStart = 0;
        rejuv.lastCharge = false;
        purchases.seen = {};
        purchases.seeded = false;
        purchases.byHero = {};
    }

    function tick() {
        try {
            if (inHideout()) {
                resetMatchState();
            } else {
                ensurePanels();
                var now = clockSeconds();
                if (now >= 0) {
                    if (lastClock > 30 && now + 5 < lastClock) { resetMatchState(); }
                    lastClock = now;
                    updateTimers(now);
                    updateUrnLead(now);
                }
                updatePurchases();
            }
        } catch (e) {
            try { $.Warning("DeadTune top bar: " + e); } catch (e2) {}
        }
        $.Schedule(CONFIG.purchases ? 0.5 : 1.0, tick);
    }

    tick();
})();
