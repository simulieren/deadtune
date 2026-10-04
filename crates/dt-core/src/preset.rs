//! Known community presets. Pinned copies are embedded; the `fetch` feature refreshes
//! them from upstream GitHub into the cache dir.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetId {
    Vanilla,
    Sqooky,
    SqookyTest,
    KaizMinspec,
    KaizExtremelow,
    BootMaxfps,
    OptilockRecommended,
    OptilockPotato,
}

#[derive(Clone, Debug)]
pub struct PresetInfo {
    pub id: PresetId,
    pub label: &'static str,
    pub author: &'static str,
    pub upstream_gameinfo: &'static str,
    pub upstream_video: Option<&'static str>,
    pub pinned_gameinfo: &'static str,
    pub pinned_video: Option<&'static str>,
}

pub fn all() -> &'static [PresetInfo] {
    todo!()
}

pub fn info(id: PresetId) -> &'static PresetInfo {
    let _ = id;
    todo!()
}
