//! Addons section of the simple view: one card per performance addon with its switch,
//! author credit, what it does and what it saves, a status line from the previewed
//! plan, and the addon's own options once it is on. Edits live in the profile and go
//! out with the normal Apply; only the texture build runs on its own button.

use std::path::PathBuf;

use dt_core::addons::install::{Action, Blocker, InstalledState};
use dt_core::addons::textures::{category_label, summary};
use dt_core::addons::{self, AddonId, AddonInfo, Kind, Source, particles};
use dt_core::addons::{Factor, TextureCategory, TextureDownscale};
use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, vec2};

use crate::simple::{caption, card_title, switch};
use crate::state::{AppState, Status};
use crate::theme::{self, ACCENT, BAD, GOOD, WARN, WEAK};

enum Edit {
    Enable(AddonId, bool),
    Expand(Option<AddonId>),
    Particle(&'static str, bool),
    AllParticles(bool),
    Blur {
        hud: bool,
        menu: bool,
    },
    Textures(TextureDownscale),
    Import(PathBuf),
    #[cfg(feature = "fetch")]
    Fetch(AddonId),
    Build,
    Retry,
}

pub fn addons(ui: &mut Ui, state: &mut AppState) {
    let mut edits = Vec::new();
    if let Some(e) = state.addons_error().map(str::to_owned) {
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.0, BAD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("DeadTune couldn't check your addons").color(BAD).strong());
                ui.label(format!("Reason: {e}"));
                ui.label(
                    RichText::new(
                        "Nothing was changed. Your other settings still apply normally. If Steam is \
                         updating Deadlock, wait for it to finish; if the reason mentions game files, \
                         run Steam > Deadlock > Properties > Installed Files > Verify integrity. Then press Retry.",
                    )
                    .color(WEAK),
                );
                if ui.button("Retry").clicked() {
                    edits.push(Edit::Retry);
                }
            });
        ui.add_space(10.0);
    }
    let states = state.addon_states();
    for info in addons::all() {
        let installed = states.get(&info.id);
        card(ui, state, info, installed, &mut edits);
        ui.add_space(12.0);
    }
    import_card(ui, state, &mut edits);
    ui.add_space(10.0);
    ui.label(
        RichText::new(
            "Each addon is its own pakNN_dir.vpk in game/citadel/addons, written and removed only by DeadTune. \
             Other mods in that folder are never touched. All addons come off with Ranked-safe mode.",
        )
        .small()
        .color(WEAK),
    );
    for edit in edits {
        match edit {
            Edit::Enable(id, on) => {
                state.set_addon_enabled(id, on);
                if on && info_has_options(id) {
                    state.ui.addon_expanded = Some(id);
                }
            }
            Edit::Expand(id) => state.ui.addon_expanded = id,
            Edit::Particle(group, visible) => state.set_particle_group(group, visible),
            Edit::AllParticles(visible) => {
                for g in particles::GROUPS {
                    state.set_particle_group(g.id, visible);
                }
            }
            Edit::Blur { hud, menu } => state.set_blur(hud, menu),
            Edit::Textures(cfg) => state.set_textures(cfg),
            Edit::Import(path) => {
                state.status = Some(match state.import_addon(&path) {
                    Ok(ids) => {
                        state.ui.addon_import_path.clear();
                        let names: Vec<&str> =
                            ids.iter().map(|id| addons::info(*id).name).collect();
                        Status::Info(format!("Imported {}.", names.join(", ")))
                    }
                    Err(e) => Status::Error(format!("import: {e}")),
                });
            }
            #[cfg(feature = "fetch")]
            Edit::Fetch(id) => {
                state.status = Some(match state.fetch_addon(id) {
                    Ok(()) => Status::Info(format!("Downloaded {}.", addons::info(id).name)),
                    Err(e) => Status::Error(format!("download: {e}")),
                });
            }
            Edit::Build => state.start_texture_build(),
            Edit::Retry => state.retry_addons(),
        }
    }
}

fn info_has_options(id: AddonId) -> bool {
    !matches!(addons::info(id).kind, Kind::Toggle)
}

/// Colour and text for the card's status line, from the previewed plan and the record.
fn status(
    state: &AppState,
    info: &AddonInfo,
    installed: Option<&InstalledState>,
) -> (Color32, String, Option<String>) {
    let on = state.profile.addons.is_enabled(info.id);
    if let Some(InstalledState::Foreign(file)) = installed {
        return (
            WARN,
            format!("{file} was replaced by another program; DeadTune leaves it alone"),
            None,
        );
    }
    let is_installed = matches!(
        installed,
        Some(InstalledState::Current(_) | InstalledState::Stale(_))
    );
    let building = info.id == AddonId::TextureDownscaler && state.texture_build.is_some();
    match state.addon_action(info.id) {
        _ if building => (ACCENT, "Building now".into(), None),
        None if on => match (state.addons_error(), &state.preview) {
            (Some(e), _) => (
                BAD,
                "Couldn't check this addon: see the message at the top".into(),
                Some(e.to_owned()),
            ),
            (None, Err(e)) => (
                BAD,
                "Can't work out changes until the problem in the action bar is fixed".into(),
                Some(e.clone()),
            ),
            (None, Ok(_)) => (WEAK, "Checking…".into(), None),
        },
        None => (WEAK, "Off".into(), None),
        Some(Action::Keep) => (GOOD, "Installed".into(), None),
        Some(Action::Write(_)) if is_installed => (ACCENT, "Rebuilds on Apply".into(), None),
        Some(Action::Write(_)) => (ACCENT, "Installs on Apply".into(), None),
        Some(Action::Remove) => (WARN, "Comes off on Apply".into(), None),
        Some(Action::Build) if matches!(installed, Some(InstalledState::Stale(_))) => (
            WARN,
            "Installed, but the game updated since: rebuild it".into(),
            None,
        ),
        Some(Action::Build) if is_installed => (
            WARN,
            "Installed with other settings: rebuild to match".into(),
            None,
        ),
        Some(Action::Build) => (ACCENT, "Press Build to make the pak".into(), None),
        Some(Action::Unavailable(Blocker::NotDownloaded)) => {
            (WARN, "Needs the upstream file first".into(), None)
        }
        Some(Action::Unavailable(Blocker::GameFiles(e))) => {
            (BAD, "Game files unreadable".into(), Some(e.clone()))
        }
    }
}

fn card(
    ui: &mut Ui,
    state: &AppState,
    info: &AddonInfo,
    installed: Option<&InstalledState>,
    edits: &mut Vec<Edit>,
) {
    let on = state.profile.addons.is_enabled(info.id);
    let expanded = state.ui.addon_expanded == Some(info.id);
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if switch(ui, on).clicked() {
                edits.push(Edit::Enable(info.id, !on));
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    card_title(ui, info.name);
                    ui.add_space(4.0);
                    ui.hyperlink_to(
                        RichText::new(format!("by {}", info.author))
                            .small()
                            .color(WEAK),
                        info.credit_url,
                    )
                    .on_hover_text(info.credit_url);
                });
                ui.label(RichText::new(info.description).color(WEAK));
                ui.label(
                    RichText::new(format!("Gains: {}", info.benefit))
                        .small()
                        .color(WEAK),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let (color, text, hover) = status(state, info, installed);
                let label = ui.label(RichText::new(text).color(color).strong());
                if let Some(hover) = hover {
                    label.on_hover_text(hover);
                }
            });
        });
        for c in state.addon_conflicts(info.id) {
            ui.colored_label(
                WARN,
                format!(
                    "{} also changes {}; only one of them can win.",
                    c.addon
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    c.paths.join(", ")
                ),
            );
        }
        if on
            && matches!(
                state.addon_action(info.id),
                Some(Action::Unavailable(Blocker::NotDownloaded))
            )
        {
            download_hint(ui, info, edits);
        }
        if !on || !info_has_options(info.id) {
            return;
        }
        ui.add_space(6.0);
        let toggle = ui
            .selectable_label(expanded, if expanded { "Hide options" } else { "Options" })
            .on_hover_text("What this addon changes, in detail");
        if toggle.clicked() {
            edits.push(Edit::Expand((!expanded).then_some(info.id)));
        }
        if !expanded {
            return;
        }
        ui.add_space(4.0);
        match info.kind {
            Kind::ParticleGroups => particle_options(ui, state, edits),
            Kind::GeneratedCss => blur_options(ui, state, edits),
            Kind::Textures => texture_options(ui, state, edits),
            Kind::Toggle => {}
        }
    });
}

fn download_hint(ui: &mut Ui, info: &AddonInfo, edits: &mut Vec<Edit>) {
    let Source::Upstream { url, file, .. } = info.source else {
        return;
    };
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        #[cfg(feature = "fetch")]
        if ui.button("Download").on_hover_text(url).clicked() {
            edits.push(Edit::Fetch(info.id));
        }
        #[cfg(not(feature = "fetch"))]
        let _ = edits;
        ui.label(RichText::new(format!("Get {file} from")).color(WEAK));
        ui.hyperlink_to("Sqooky's OptimizationLock on GitHub", url);
        ui.label(RichText::new("and import it below.").color(WEAK));
    });
}

fn particle_options(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let keep = &state.profile.addons.keep_particles;
    ui.horizontal(|ui| {
        caption(ui, "Hide screen-edge effects for");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("Show all").clicked() {
                edits.push(Edit::AllParticles(true));
            }
            if ui.small_button("Hide all").clicked() {
                edits.push(Edit::AllParticles(false));
            }
        });
    });
    let columns = ((ui.available_width() / 250.0) as usize).clamp(1, 3);
    egui::Grid::new("particle_groups")
        .num_columns(columns)
        .spacing([18.0, 2.0])
        .show(ui, |ui| {
            for (i, g) in particles::GROUPS.iter().enumerate() {
                let mut hidden = !keep.contains(g.id);
                let label = format!("{} ({})", g.label, g.paths.len());
                if ui.checkbox(&mut hidden, label).changed() {
                    edits.push(Edit::Particle(g.id, !hidden));
                }
                if (i + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    let hidden = particles::GROUPS.len() - keep.len().min(particles::GROUPS.len());
    ui.label(
        RichText::new(format!(
            "{hidden} of {} groups hidden. Nothing hidden means no pak is installed.",
            particles::GROUPS.len()
        ))
        .small()
        .color(WEAK),
    );
}

fn blur_options(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let blur = state.profile.addons.blur;
    caption(ui, "Turn off blur behind");
    ui.horizontal(|ui| {
        let mut hud = blur.hud;
        let mut menu = blur.menu;
        let a = ui
            .checkbox(&mut hud, "HUD panels (minimap frame and friends)")
            .changed();
        let b = ui.checkbox(&mut menu, "Menus").changed();
        if a || b {
            edits.push(Edit::Blur { hud, menu });
        }
    });
    ui.label(
        RichText::new(
            "Made from your game's own stylesheet, so it is rebuilt automatically after every game update.",
        )
        .small()
        .color(WEAK),
    );
}

fn texture_options(ui: &mut Ui, state: &AppState, edits: &mut Vec<Edit>) {
    let cfg = &state.profile.addons.textures;
    ui.horizontal(|ui| {
        caption(ui, "Texture size");
        for (factor, label) in [(Factor::Half, "1/2"), (Factor::Quarter, "1/4")] {
            if ui.selectable_label(cfg.factor == factor, label).clicked() && cfg.factor != factor {
                edits.push(Edit::Textures(TextureDownscale {
                    factor,
                    ..cfg.clone()
                }));
            }
        }
    });
    caption(ui, "Downscale");
    ui.horizontal_wrapped(|ui| {
        for cat in TextureCategory::ALL {
            let mut on = cfg.categories.contains(&cat);
            if ui.checkbox(&mut on, category_label(cat)).changed() {
                let mut next = cfg.clone();
                if on {
                    next.categories.insert(cat);
                } else {
                    next.categories.remove(&cat);
                }
                edits.push(Edit::Textures(next));
            }
        }
    });
    let mut sharp = cfg.exclude_lighting;
    if ui
        .checkbox(&mut sharp, "Keep lighting textures sharp (recommended)")
        .on_hover_text("Downscaled lightmaps make the map look bright and flat.")
        .changed()
    {
        edits.push(Edit::Textures(TextureDownscale {
            exclude_lighting: sharp,
            ..cfg.clone()
        }));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let installed = matches!(
            state.addon_action(AddonId::TextureDownscaler),
            Some(Action::Keep)
        );
        let label = if installed { "Rebuild" } else { "Build" };
        match &state.texture_build {
            Some(build) => {
                let p = &build.progress;
                let fraction = if p.total == 0 {
                    0.0
                } else {
                    p.done as f32 / p.total as f32
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width(220.0)
                        .text(format!("{}/{}", p.done, p.total)),
                );
                if ui.small_button("Cancel").clicked() {
                    build.cancel();
                }
                ui.label(
                    RichText::new(format!("{} reduced; {}", p.stats.reduced, p.current))
                        .small()
                        .color(WEAK),
                );
            }
            None => {
                if ui
                    .add_enabled(
                        !installed,
                        egui::Button::new(RichText::new(label).strong()).min_size(vec2(90.0, 28.0)),
                    )
                    .on_hover_text(
                        "Reads every texture from the game archives; takes a few minutes.",
                    )
                    .clicked()
                {
                    edits.push(Edit::Build);
                }
            }
        }
    });
    match &state.last_texture_build {
        Some(Ok(stats)) => {
            let skipped: Vec<String> = stats
                .skipped
                .iter()
                .map(|(reason, n)| format!("{n} {reason:?}"))
                .collect();
            ui.label(RichText::new(format!("Last build: {}.", summary(stats))).color(GOOD));
            if !skipped.is_empty() {
                ui.label(
                    RichText::new(format!("Skipped: {}.", skipped.join(", ")))
                        .small()
                        .color(WEAK),
                );
            }
        }
        Some(Err(e)) => {
            ui.colored_label(BAD, format!("Last build failed: {e}"));
        }
        None => {}
    }
    ui.label(
        RichText::new(
            "Originals are never modified: DeadTune writes its own pak with the smaller copies. \
             Build again after a game update or after changing the options above.",
        )
        .small()
        .color(WEAK),
    );
}

fn import_card(ui: &mut Ui, state: &mut AppState, edits: &mut Vec<Edit>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "Import downloaded files");
        ui.label(
            RichText::new(
                "Have the upstream .vpk files already? Paste the path to one file or to the extracted folder. \
                 DeadTune recognises them by content, whatever they are called.",
            )
            .color(WEAK),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.ui.addon_import_path)
                    .desired_width(460.0)
                    .hint_text("C:\\Downloads\\Various Addons Relating to Performance"),
            );
            let path = state.ui.addon_import_path.trim();
            if ui.add_enabled(!path.is_empty(), egui::Button::new("Import")).clicked() {
                edits.push(Edit::Import(PathBuf::from(path)));
            }
        });
    });
}
