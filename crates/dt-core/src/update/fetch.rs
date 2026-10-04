use std::io::Read;
use std::time::Duration;

use super::{Asset, Channel, Manifest, STABLE_MANIFEST_URL, TESTING_MANIFEST_URL, UpdateError};

const MANIFEST_LIMIT: u64 = 1024 * 1024;
/// The manifest is unsigned, so its `size` is untrusted until `verify`.
const MAX_ASSET: u64 = 512 * 1024 * 1024;
/// Lets an oversized body reach `verify` as a size mismatch instead of a read error.
const DOWNLOAD_SLACK: u64 = 64 * 1024;

/// Follows redirects (ureq's default of 10), which GitHub's `releases/.../download`
/// URLs need.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("deadtune/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

pub fn fetch_manifest(channel: Channel) -> Result<Manifest, UpdateError> {
    let url = match channel {
        Channel::Stable => STABLE_MANIFEST_URL,
        Channel::Testing => TESTING_MANIFEST_URL,
    };
    let text = agent(Duration::from_secs(15))
        .get(url)
        .call()?
        .body_mut()
        .with_config()
        .limit(MANIFEST_LIMIT)
        .read_to_string()?;
    serde_json::from_str(&text).map_err(|e| UpdateError::Manifest(e.to_string()))
}

/// Downloads `asset.url`, calling `progress(done, total)`; does not verify.
pub fn download(asset: &Asset, mut progress: impl FnMut(u64, u64)) -> Result<Vec<u8>, UpdateError> {
    if asset.size > MAX_ASSET {
        return Err(UpdateError::Manifest(format!(
            "asset size {} is implausible",
            asset.size
        )));
    }
    let cap = asset.size + DOWNLOAD_SLACK;
    let mut response = agent(Duration::from_secs(600)).get(&asset.url).call()?;
    let mut reader = response.body_mut().as_reader().take(cap);
    let mut bytes = Vec::with_capacity(asset.size as usize);
    let mut chunk = [0u8; 64 * 1024];
    progress(0, asset.size);
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        progress(bytes.len() as u64, asset.size);
    }
    Ok(bytes)
}
