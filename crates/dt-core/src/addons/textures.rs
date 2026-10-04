//! The low-VRAM texture downscaler as an addon. The work is `crate::texture`: it reads
//! the game's own `.vtex_c` files, drops their largest mips and streams the copies into
//! our pak (chunked past 256 MiB). This module only names what the install record and
//! the UI need: a fingerprint for the config, friendly labels, and an owned progress.

use crate::texture::{Category, Stats, TextureDownscale};

pub fn is_default(cfg: &TextureDownscale) -> bool {
    *cfg == TextureDownscale::default()
}

/// Stable text for the install record's input fingerprint.
pub fn fingerprint(cfg: &TextureDownscale) -> String {
    let cats: Vec<&str> = cfg.categories.iter().map(|c| c.label()).collect();
    format!(
        "factor={:?};categories={};lighting={}",
        cfg.factor,
        cats.join(","),
        if cfg.exclude_lighting {
            "sharp"
        } else {
            "downscaled"
        }
    )
}

pub fn category_label(cat: Category) -> &'static str {
    match cat {
        Category::World => "World and props",
        Category::Heroes => "Heroes",
        Category::Particles => "Particles",
        Category::Ui => "UI and HUD",
        Category::Other => "Everything else",
    }
}

/// `texture::Progress` without the borrows, for a channel between threads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// VPK path of the texture just processed.
    pub current: String,
    pub stats: Stats,
}

impl From<crate::texture::Progress<'_>> for Progress {
    fn from(p: crate::texture::Progress<'_>) -> Progress {
        Progress {
            done: p.done,
            total: p.total,
            current: p.path.to_string(),
            stats: p.stats.clone(),
        }
    }
}

/// One line for the status bar and the CLI: what a build did.
pub fn summary(stats: &Stats) -> String {
    let skipped: usize = stats.skipped.values().sum();
    let mb = |b: u64| (b as f64 / (1024.0 * 1024.0)).round() as u64;
    format!(
        "{} of {} textures reduced ({} skipped), {} MB down to {} MB",
        stats.reduced,
        stats.textures,
        skipped,
        mb(stats.bytes_before),
        mb(stats.bytes_after)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::Factor;

    #[test]
    fn fingerprint_follows_every_field() {
        let base = TextureDownscale::default();
        assert!(is_default(&base));
        let mut f = base.clone();
        f.factor = Factor::Quarter;
        let mut c = base.clone();
        c.categories.insert(Category::Ui);
        let mut l = base.clone();
        l.exclude_lighting = false;
        let prints = [
            fingerprint(&base),
            fingerprint(&f),
            fingerprint(&c),
            fingerprint(&l),
        ];
        for (i, a) in prints.iter().enumerate() {
            for (j, b) in prints.iter().enumerate() {
                assert_eq!(a == b, i == j, "{a} vs {b}");
            }
        }
    }

    #[test]
    fn summary_reads_in_megabytes() {
        let stats = Stats {
            textures: 10,
            reduced: 4,
            bytes_before: 3 << 20,
            bytes_after: 1 << 20,
            skipped: [(crate::texture::SkipReason::Lighting, 6)].into(),
        };
        assert_eq!(
            summary(&stats),
            "4 of 10 textures reduced (6 skipped), 3 MB down to 1 MB"
        );
    }
}
