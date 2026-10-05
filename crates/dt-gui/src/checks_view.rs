//! The simple view's System check page: a summary, what needs attention with the fix,
//! then every check grouped by area in plain words. The doctor's own names and details
//! stay in tooltips and the copied report.

use dt_core::doctor::{Check, CheckStatus};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, vec2,
};

use crate::icons::{self, Icon};
use crate::state::{AppState, Status};
use crate::theme::{self, ACCENT, BAD, BORDER, CARD, GOOD, TEXT, WARN, WEAK};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Game,
    Addons,
    Windows,
    DeadTune,
}

impl Area {
    const ALL: [Area; 4] = [Area::Game, Area::Addons, Area::Windows, Area::DeadTune];

    fn label(self) -> &'static str {
        match self {
            Area::Game => "Deadlock files",
            Area::Addons => "HUD and addons",
            Area::Windows => "Windows and hardware",
            Area::DeadTune => "DeadTune",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Area::Game => Icon::Folder,
            Area::Addons => Icon::Addons,
            Area::Windows => Icon::Display,
            Area::DeadTune => Icon::Safety,
        }
    }
}

/// What a check means to a player: a plain title and one sentence on what it looks at.
#[derive(Clone, Copy, Debug)]
pub struct CheckInfo {
    pub name: &'static str,
    pub area: Area,
    pub title: &'static str,
    /// Whether the doctor's detail reads as plain words (a refresh rate, not a file path).
    pub plain_detail: bool,
    pub about: &'static str,
}

const fn info(
    name: &'static str,
    area: Area,
    title: &'static str,
    plain_detail: bool,
    about: &'static str,
) -> CheckInfo {
    CheckInfo {
        name,
        area,
        title,
        plain_detail,
        about,
    }
}

/// Keyed by the doctor's check names; a test fails when a check has no entry.
#[rustfmt::skip]
pub static INFO: &[CheckInfo] = &[
    info("Find Deadlock", Area::Game, "Deadlock install", false, "DeadTune looks for your Deadlock folder through Steam."),
    info("Read gameinfo.gi", Area::Game, "Game settings file", false, "The file where the game keeps its performance settings."),
    info("Line endings", Area::Game, "Settings file format", false, "DeadTune keeps the file's line endings exactly as Valve wrote them."),
    info("Brace balance", Area::Game, "Settings file structure", false, "Every bracket in the settings file must be closed, or the game can't read it."),
    info("Lossless edit", Area::Game, "Safe editing", false, "A test edit must change only DeadTune's part and leave every other byte alone."),
    info("Read convars", Area::Game, "Performance settings", false, "The performance settings inside the game's settings file."),
    info("Read video.txt", Area::Game, "Video settings", false, "The video settings you picked in the game's menu."),
    info("Game build id", Area::Game, "Game version", true, "Which game update you have, so DeadTune notices new ones."),
    info("Write cfg folder", Area::Game, "Instant changes", false, "The small files the game loads for instant changes."),
    info("Game running", Area::Game, "Deadlock running", true, "Whether the game is open now. File changes load the next time it starts."),
    info("Steam launch options", Area::Game, "Steam launch options", true, "Launch options you set in Steam can override what DeadTune sets."),
    info("Game archive (HUD)", Area::Addons, "Game files for the HUD", false, "DeadTune needs the game's own HUD files to build your HUD."),
    info("HUD addon", Area::Addons, "HUD layout addon", true, "DeadTune's HUD addon should be installed and match your HUD settings."),
    info("HUD search path", Area::Addons, "Addon loading", false, "The game must be set up to load mods from its addons folder."),
    info("HUD conflicts", Area::Addons, "Other HUD mods", false, "Another mod that replaces the same HUD files would undo your layout."),
    info("Performance addons", Area::Addons, "Performance addons", true, "The performance addons you turned on."),
    info("Addons after update", Area::Addons, "Addons and game updates", false, "Addons are rebuilt after a game update so they keep working."),
    info("Addon files", Area::Addons, "Addon files", false, "Addon files DeadTune installed should not be changed by anything else."),
    info("Other mods", Area::Addons, "Mods from other tools", true, "Mods DeadTune did not install. If FPS drops, try without them."),
    info("GPU for Deadlock", Area::Windows, "Graphics card for Deadlock", true, "On a laptop, Deadlock should run on the fast graphics card, not the built-in one."),
    info("Power plan", Area::Windows, "Power plan", true, "Power-saving modes slow the CPU down while you play."),
    info("Refresh rate", Area::Windows, "Monitor refresh rate", true, "Your monitor should run at the highest refresh rate it supports."),
    info("Background recording", Area::Windows, "Background recording", true, "Xbox Game Bar recording in the background costs frames the whole time."),
    info("RAM", Area::Windows, "Memory (RAM)", true, "Two memory sticks running at their rated speed give steadier FPS."),
    info("Overlays", Area::Windows, "Overlays", true, "Apps that draw over the game, like Discord or recorders, each cost a little FPS."),
    info("GPU driver", Area::Windows, "Graphics driver", true, "A recent graphics driver is faster and more stable in new games."),
    info("Game Mode", Area::Windows, "Game Mode", true, "Windows Game Mode puts the game ahead of background tasks."),
    info("Memory integrity", Area::Windows, "Memory integrity", true, "A Windows security feature that can cost a few percent of FPS. For information."),
    info("Windowed game optimizations", Area::Windows, "Windowed game optimizations", true, "Lowers input lag when you play in a borderless window."),
    info("GPU scheduling", Area::Windows, "GPU scheduling", true, "Hardware-accelerated GPU scheduling. For information."),
    info("Game drive", Area::Windows, "Game drive", true, "Deadlock loads faster and stutters less on an SSD."),
    info("Free disk space", Area::Windows, "Free disk space", true, "Game updates and shader caches need room on the game's drive."),
    info("Steam background work", Area::Windows, "Steam background work", true, "Steam processing shaders or downloading takes CPU and disk while you play."),
    info("DeadTune data folder", Area::DeadTune, "DeadTune's folder", false, "Where DeadTune keeps its settings and backups."),
    info("Backups", Area::DeadTune, "Backups", false, "Your original game files, saved so every change can be undone."),
    info("Live console (netcon)", Area::DeadTune, "Live console", false, "An optional faster link to the running game. DeadTune works without it."),
];

/// Windows checks on made-up facts, a mix of good and bad: tests, and screenshots on a
/// Mac with `DEADTUNE_FAKE_WINDOWS=1`.
pub fn sample_windows_checks() -> Vec<Check> {
    use dt_core::winfps::facts::*;
    let facts = WindowsFacts {
        gpus: vec![
            Gpu {
                name: "NVIDIA GeForce RTX 3060 Laptop GPU".into(),
                vendor_id: Some(VENDOR_NVIDIA),
                vram_mib: Some(6144),
                driver_date: Some(Date {
                    year: 2025,
                    month: 11,
                    day: 2,
                }),
                driver_version: Some("32.0.15.8157".into()),
            },
            Gpu {
                name: "Intel(R) UHD Graphics".into(),
                vendor_id: Some(VENDOR_INTEL),
                vram_mib: Some(128),
                driver_date: Some(Date {
                    year: 2026,
                    month: 6,
                    day: 1,
                }),
                driver_version: Some("31.0.101.5333".into()),
            },
        ],
        deadlock_gpu_preference: Some(GpuPreference::HighPerformance),
        power: Some(PowerFacts {
            plan: PowerPlan::Balanced,
            mode: Some(PowerMode::BestPowerEfficiency),
            on_battery: false,
            battery_saver: false,
        }),
        displays: vec![Display {
            name: "Built-in display".into(),
            current_hz: 165,
            max_hz: 165,
        }],
        background_recording: Some(true),
        game_mode: Some(true),
        windowed_optimizations: Some(true),
        hags: Some(true),
        memory_integrity: Some(false),
        memory: vec![
            MemoryStick {
                capacity_mib: 8192,
                rated_mts: Some(3200),
                configured_mts: Some(3200),
            },
            MemoryStick {
                capacity_mib: 8192,
                rated_mts: Some(3200),
                configured_mts: Some(3200),
            },
        ],
        overlays: Some(vec!["Discord"]),
        game_drive: Some(DriveKind::Ssd),
        game_drive_free_mib: Some(6 * 1024),
        shader_processing: Some(false),
    };
    dt_core::winfps::evaluate::checks(
        &facts,
        Date {
            year: 2026,
            month: 10,
            day: 5,
        },
    )
}

pub fn info_for(check: &Check) -> CheckInfo {
    INFO.iter()
        .find(|i| i.name == check.name)
        .copied()
        .unwrap_or(CheckInfo {
            name: check.name,
            area: Area::Game,
            title: check.name,
            plain_detail: false,
            about: "",
        })
}

/// The page's headline for a finished run.
pub fn headline(checks: &[Check]) -> (CheckStatus, String) {
    let fails = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .count();
    let warns = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Warn)
        .count();
    let text = match (fails + warns, fails) {
        (0, _) => "Everything looks good".to_string(),
        (1, _) => "1 thing needs your attention".to_string(),
        (n, _) => format!("{n} things need your attention"),
    };
    let worst = if fails > 0 {
        CheckStatus::Fail
    } else if warns > 0 {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };
    (worst, text)
}

fn color(status: CheckStatus) -> Color32 {
    match status {
        CheckStatus::Pass => GOOD,
        CheckStatus::Warn => WARN,
        CheckStatus::Fail => BAD,
    }
}

/// A check mark, an exclamation or a cross in a filled circle.
fn status_glyph(ui: &mut Ui, status: CheckStatus, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let c = rect.center();
    let r = size / 2.0;
    let tone = color(status);
    let painter = ui.painter();
    painter.circle_filled(c, r, tone.gamma_multiply(0.18));
    let stroke = Stroke::new((size / 9.0).max(1.5), tone);
    let u = r * 0.42;
    match status {
        CheckStatus::Pass => {
            painter.add(egui::Shape::line(
                vec![
                    c + vec2(-u, 0.0),
                    c + vec2(-u * 0.25, u * 0.75),
                    c + vec2(u, -u * 0.7),
                ],
                stroke,
            ));
        }
        CheckStatus::Warn => {
            painter.line_segment([c + vec2(0.0, -u), c + vec2(0.0, u * 0.25)], stroke);
            painter.circle_filled(c + vec2(0.0, u * 0.85), stroke.width * 0.6, tone);
        }
        CheckStatus::Fail => {
            painter.line_segment(
                [c + vec2(-u * 0.8, -u * 0.8), c + vec2(u * 0.8, u * 0.8)],
                stroke,
            );
            painter.line_segment(
                [c + vec2(-u * 0.8, u * 0.8), c + vec2(u * 0.8, -u * 0.8)],
                stroke,
            );
        }
    }
    response
}

pub fn page(ui: &mut Ui, state: &mut AppState) {
    if state.checks.is_none() && !state.checks_running() {
        state.run_checks();
    }
    if state.checks_running() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    summary(ui, state);
    let Some(checks) = state.checks.clone() else {
        return;
    };
    let attention: Vec<&Check> = {
        let mut v: Vec<&Check> = checks
            .iter()
            .filter(|c| c.status != CheckStatus::Pass)
            .collect();
        v.sort_by_key(|c| c.status != CheckStatus::Fail);
        v
    };
    if !attention.is_empty() {
        ui.add_space(14.0);
        section_caption(ui, "Needs attention");
        for check in attention {
            problem_card(ui, check);
        }
    }
    ui.add_space(14.0);
    section_caption(ui, "All checks");
    let wide = ui.available_width() >= 820.0;
    let areas: Vec<(Area, Vec<&Check>)> = Area::ALL
        .into_iter()
        .map(|area| {
            (
                area,
                checks
                    .iter()
                    .filter(|c| info_for(c).area == area)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, list)| !list.is_empty())
        .collect();
    if wide {
        let gap = 12.0;
        let width = (ui.available_width() - gap) / 2.0;
        let sizes: Vec<usize> = areas.iter().map(|(_, list)| list.len()).collect();
        let [left, right] = columns(&sizes);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for column in [left, right] {
                ui.vertical(|ui| {
                    ui.set_width(width);
                    for i in column {
                        let (area, list) = &areas[i];
                        area_card(ui, *area, list);
                        ui.add_space(gap);
                    }
                });
            }
        });
    } else {
        for (area, list) in &areas {
            area_card(ui, *area, list);
            ui.add_space(12.0);
        }
    }
}

/// Splits cards between two columns so their heights come out close; each column keeps
/// the cards in their original order.
fn columns(sizes: &[usize]) -> [Vec<usize>; 2] {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i]));
    let mut cols: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    let mut heights = [0usize; 2];
    for i in order {
        // A card's header counts like two rows.
        let side = usize::from(heights[1] < heights[0]);
        heights[side] += sizes[i] + 2;
        cols[side].push(i);
    }
    cols.iter_mut().for_each(|c| c.sort_unstable());
    if cols[1].first() < cols[0].first() && !cols[1].is_empty() {
        cols.swap(0, 1);
    }
    cols
}

fn section_caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .family(theme::semibold())
            .color(WEAK),
    );
    ui.add_space(4.0);
}

/// Room the summary text leaves for Check again and Copy report.
const SUMMARY_BUTTONS: f32 = 220.0;

fn summary(ui: &mut Ui, state: &mut AppState) {
    let running = state.checks_running();
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            match (&state.checks, running) {
                (None, _) => {
                    ui.add(egui::Spinner::new().size(36.0).color(ACCENT));
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width() - SUMMARY_BUTTONS);
                        ui.label(RichText::new("Checking your setup…").size(18.0).family(theme::semibold()).color(TEXT));
                        ui.label(RichText::new("Reading the game files and your Windows settings. Nothing is changed.").color(WEAK));
                    });
                }
                (Some(checks), _) => {
                    let (worst, text) = headline(checks);
                    status_glyph(ui, worst, 40.0);
                    ui.add_space(8.0);
                    let passed = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width() - SUMMARY_BUTTONS);
                        ui.label(RichText::new(text).size(18.0).family(theme::semibold()).color(TEXT));
                        let when = state
                            .checks_at
                            .map(|at| format!(" · checked at {}", crate::live_status::clock(at)))
                            .unwrap_or_default();
                        let sub = if running {
                            "Checking again…".to_string()
                        } else {
                            format!("{passed} of {} checks passed{when}. Nothing was changed.", checks.len())
                        };
                        ui.label(RichText::new(sub).color(WEAK));
                    });
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("Copy report")
                    .on_hover_text(
                        "Plain text for a bug report: DeadTune version, the gameinfo.gi SearchPaths block, what is in \
                         game/citadel/addons and who owns it, DeadTune's records, the launch guard, the last launch \
                         arguments, the last 80 console.log lines and a read-back of every installed pak.",
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(state.diagnostic_report());
                    state.status = Some(Status::Info(
                        "Report copied. Paste it into your notes or a bug report.".into(),
                    ));
                }
                let label = if running { "Checking…" } else { "Check again" };
                if ui
                    .add_enabled(!running, egui::Button::new(label))
                    .clicked()
                {
                    state.run_checks();
                }
            });
        });
    });
}

fn problem_card(ui: &mut Ui, check: &Check) {
    let info = info_for(check);
    let tone = color(check.status);
    let frame = egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, tone.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin {
            left: 16,
            right: 14,
            top: 12,
            bottom: 12,
        });
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            status_glyph(ui, check.status, 22.0);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(info.title)
                            .size(15.0)
                            .family(theme::semibold())
                            .color(TEXT),
                    );
                    let pill = match check.status {
                        CheckStatus::Fail => "Problem",
                        _ => "Worth a look",
                    };
                    crate::widgets::badge(ui, pill, tone);
                });
                if !info.about.is_empty() {
                    ui.label(RichText::new(info.about).color(WEAK));
                }
                if let Some(fix) = &check.fix {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            RichText::new("What to do:")
                                .family(theme::semibold())
                                .color(TEXT),
                        );
                        ui.label(RichText::new(fix).color(TEXT));
                    });
                }
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!("Found: {}", check.detail))
                        .size(11.5)
                        .color(WEAK.gamma_multiply(0.85)),
                )
                .on_hover_text(check.name);
                if let Some(uri) = check.link {
                    ui.add_space(4.0);
                    ui.hyperlink_to(RichText::new("Open the Windows setting").color(ACCENT), uri);
                }
            });
        });
    });
    let r = response.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_size(r.min + vec2(0.0, 10.0), vec2(3.0, r.height() - 20.0)),
        CornerRadius::same(2),
        tone,
    );
    ui.add_space(8.0);
}

fn area_card(ui: &mut Ui, area: Area, list: &[&Check]) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let ok = list
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), rect, area.icon(), ACCENT);
            ui.label(
                RichText::new(area.label())
                    .size(14.5)
                    .family(theme::semibold())
                    .color(TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let all_ok = ok == list.len();
                ui.label(
                    RichText::new(format!("{ok} of {} OK", list.len()))
                        .size(11.5)
                        .color(if all_ok { GOOD } else { WARN }),
                );
            });
        });
        ui.add_space(6.0);
        for (i, check) in list.iter().enumerate() {
            if i > 0 {
                let (line, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .rect_filled(line, CornerRadius::ZERO, BORDER.gamma_multiply(0.6));
            }
            check_row(ui, check);
        }
    });
}

fn check_row(ui: &mut Ui, check: &Check) {
    let info = info_for(check);
    let response = ui
        .horizontal(|ui| {
            ui.set_min_height(30.0);
            status_glyph(ui, check.status, 16.0);
            ui.add_space(2.0);
            let title = RichText::new(info.title).color(if check.status == CheckStatus::Pass {
                TEXT
            } else {
                color(check.status)
            });
            ui.label(title);
            if check.status != CheckStatus::Pass {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("See above")
                            .size(11.5)
                            .color(WEAK.gamma_multiply(0.8)),
                    );
                });
            } else if info.plain_detail {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&check.detail)
                                .size(11.5)
                                .color(WEAK.gamma_multiply(0.8)),
                        )
                        .truncate(),
                    );
                });
            }
        })
        .response;
    let mut tip = String::new();
    if !info.about.is_empty() {
        tip.push_str(info.about);
        tip.push_str("\n\n");
    }
    tip.push_str(&format!("Found: {}\nCheck: {}", check.detail, check.name));
    response.on_hover_text(tip);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &'static str, status: CheckStatus) -> Check {
        Check {
            name,
            status,
            detail: String::new(),
            fix: None,
            link: None,
        }
    }

    #[test]
    fn headline_counts_what_needs_attention() {
        let ok = [check("Backups", CheckStatus::Pass)];
        assert_eq!(
            headline(&ok),
            (CheckStatus::Pass, "Everything looks good".into())
        );
        let one = [
            check("Backups", CheckStatus::Pass),
            check("RAM", CheckStatus::Warn),
        ];
        assert_eq!(
            headline(&one),
            (CheckStatus::Warn, "1 thing needs your attention".into())
        );
        let two = [
            check("RAM", CheckStatus::Warn),
            check("Backups", CheckStatus::Fail),
        ];
        assert_eq!(
            headline(&two),
            (CheckStatus::Fail, "2 things need your attention".into())
        );
    }

    #[test]
    fn every_check_a_run_produces_has_plain_words() {
        let (dir, _state) = crate::state::testutil::state();
        let paths = crate::state::testutil::fake_install().1;
        let mut checks = dt_core::doctor::run(Some(&paths), &dir.path().join("data"));
        checks.extend(dt_core::doctor::run(None, &dir.path().join("data")));
        checks.extend(sample_windows_checks());
        let names: std::collections::BTreeSet<&str> = checks.iter().map(|c| c.name).collect();
        assert!(names.len() >= 25, "{names:?}");
        let missing: Vec<&str> = names
            .into_iter()
            .filter(|n| !INFO.iter().any(|i| i.name == *n))
            .collect();
        assert!(missing.is_empty(), "add plain words for {missing:?}");
    }

    #[test]
    fn columns_balance_and_keep_order() {
        assert_eq!(columns(&[10, 6, 12, 3]), [vec![0, 1], vec![2, 3]]);
        assert_eq!(columns(&[5]), [vec![0], vec![]]);
        assert_eq!(columns(&[]), [Vec::<usize>::new(), vec![]]);
    }

    #[test]
    fn info_names_are_unique() {
        let mut names: Vec<&str> = INFO.iter().map(|i| i.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }
}
