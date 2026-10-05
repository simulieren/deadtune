(function () {
    "use strict";
    var CONFIG = DT_INGAME;
    var ctx = $.GetContextPanel();

    function valid(p) {
        try { return !!p && (!p.IsValid || p.IsValid()); } catch (e) { return false; }
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
    function cmd(c) {
        try { $.DispatchEvent("CitadelConCommand", c); } catch (e) {}
    }
    function fmt(v) { return (Math.round(v * 1000) / 1000).toFixed(3); }

    // The stock slider control holds a text box ("Value") and a 0..1 slider ("Slider").
    function sliderValue(row) {
        var text = find(row, "Value");
        var v = NaN;
        try { v = text && text.text !== "" ? parseFloat(text.text) : NaN; } catch (e) { v = NaN; }
        if (isNaN(v)) {
            var slider = find(row, "Slider");
            if (slider) {
                try { v = CONFIG.min + Number(slider.value) * (CONFIG.max - CONFIG.min); } catch (e) { v = NaN; }
            }
        }
        return isNaN(v) ? null : v;
    }

    var last = null;
    function poll() {
        if (!valid(ctx)) { return; }
        var row = find(ctx, CONFIG.sliderId);
        var v = row ? sliderValue(row) : null;
        if (v !== null && last !== null && Math.abs(v - last) > 0.0004) {
            cmd(CONFIG.stash + " " + fmt(CONFIG.stashOffset + v));
            cmd("host_writeconfig");
        }
        if (v !== null) { last = v; }
        $.Schedule(0.25, poll);
    }

    // The game titles a subsection from a localization token named after its id; ours
    // has none, so the title is set here.
    function title(tries) {
        var group = find(ctx, CONFIG.groupId);
        if (!group) {
            if (tries > 0) { $.Schedule(0.5, function () { title(tries - 1); }); }
            return;
        }
        try { group.SetDialogVariable("subsection_name", CONFIG.groupTitle); } catch (e) {}
        var labels = byClass(group, "SettingsSectionSubtitle");
        for (var i = 0; i < labels.length; i++) {
            try { labels[i].text = CONFIG.groupTitle; } catch (e) {}
        }
    }

    if (CONFIG.wideFov) { $.Schedule(0.5, poll); }
    if (CONFIG.groupId) { $.Schedule(0.2, function () { title(10); }); }
})();
