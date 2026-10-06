//! Pictures for the UI images page, decoded off the UI thread. Each frame the page asks for
//! the pictures it is about to draw; the ones not cached replace the worker queue, so tiles
//! scrolled past are never decoded. Finished pictures become egui textures in a cache with
//! a memory budget, least recently drawn first out.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::images::{Facts, ImageSource, Picture, render};

const WORKERS: usize = 3;
/// Texture memory the cache may hold, in RGBA bytes. A page of previews or a screen of
/// image tiles fits in a few MB; the rest only kept pictures the player had scrolled past.
const BUDGET: usize = 32 << 20;

/// A cache that keeps the most recently used values within a total weight.
pub struct Lru<K, V> {
    map: HashMap<K, (V, u64, usize)>,
    order: BTreeMap<u64, K>,
    tick: u64,
    weight: usize,
    budget: usize,
}

impl<K: Hash + Eq + Clone, V> Lru<K, V> {
    pub fn new(budget: usize) -> Lru<K, V> {
        Lru {
            map: HashMap::new(),
            order: BTreeMap::new(),
            tick: 0,
            weight: 0,
            budget,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.tick += 1;
        let (_, used, _) = self.map.get_mut(key)?;
        self.order.remove(used);
        *used = self.tick;
        self.order.insert(self.tick, key.clone());
        self.map.get(key).map(|(v, _, _)| v)
    }

    #[cfg(test)]
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Inserts `value`, then drops the least recently used others until the total fits.
    pub fn insert(&mut self, key: K, value: V, weight: usize) {
        self.tick += 1;
        if let Some((_, used, w)) = self.map.remove(&key) {
            self.order.remove(&used);
            self.weight -= w;
        }
        self.map.insert(key.clone(), (value, self.tick, weight));
        self.order.insert(self.tick, key);
        self.weight += weight;
        while self.weight > self.budget && self.order.len() > 1 {
            let (_, oldest) = self.order.pop_first().expect("non-empty");
            if let Some((_, _, w)) = self.map.remove(&oldest) {
                self.weight -= w;
            }
        }
    }

    #[cfg(test)]
    pub fn weight(&self) -> usize {
        self.weight
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Pictures waiting for a worker, and those being decoded.
#[derive(Default)]
struct Queue {
    wanted: VecDeque<Picture>,
    in_flight: HashSet<Picture>,
    closed: bool,
}

impl Queue {
    /// This frame's wishes replace the old ones, minus what a worker already has.
    fn replace(&mut self, wanted: Vec<Picture>) {
        self.wanted = wanted
            .into_iter()
            .filter(|p| !self.in_flight.contains(p))
            .collect();
    }

    fn take(&mut self) -> Option<Picture> {
        let next = self.wanted.pop_front()?;
        self.in_flight.insert(next.clone());
        Some(next)
    }
}

#[derive(Clone)]
pub enum Slot {
    Ready {
        texture: TextureHandle,
        facts: Facts,
    },
    Failed(String),
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct Thumbs {
    shared: Shared,
    rx: Receiver<(Picture, Slot, usize)>,
    cache: Lru<Picture, Slot>,
    wanted: Vec<Picture>,
    pub checker: TextureHandle,
    /// When a page last drew with these pictures (`AppState::release_pictures`).
    pub drawn: Instant,
}

impl Thumbs {
    pub fn new(ctx: &egui::Context, source: Arc<ImageSource>) -> Thumbs {
        let shared: Shared = Arc::default();
        let (tx, rx) = channel();
        for _ in 0..WORKERS {
            let (shared, tx, ctx, source) =
                (shared.clone(), tx.clone(), ctx.clone(), source.clone());
            std::thread::spawn(move || worker(&shared, &tx, &ctx, &source));
        }
        let checker = ctx.load_texture(
            "images_checker",
            ColorImage::from_rgba_unmultiplied(
                [2, 2],
                &[
                    58, 61, 68, 255, 44, 47, 53, 255, 44, 47, 53, 255, 58, 61, 68, 255,
                ],
            ),
            TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::Repeat,
                mipmap_mode: None,
            },
        );
        Thumbs {
            shared,
            rx,
            cache: Lru::new(BUDGET),
            wanted: Vec::new(),
            checker,
            drawn: Instant::now(),
        }
    }

    /// Takes in what the workers finished since the last frame.
    pub fn poll(&mut self) {
        for (picture, slot, weight) in self.rx.try_iter() {
            self.cache.insert(picture, slot, weight);
        }
    }

    /// The picture if it is ready (or failed); otherwise it is queued for this frame.
    pub fn get(&mut self, picture: &Picture) -> Option<Slot> {
        match self.cache.get(picture) {
            Some(slot) => Some(slot.clone()),
            None => {
                if !self.wanted.contains(picture) {
                    self.wanted.push(picture.clone());
                }
                None
            }
        }
    }

    /// Hands this frame's wishes to the workers, in the order they were asked for.
    pub fn end_frame(&mut self) {
        self.drawn = Instant::now();
        let wanted = std::mem::take(&mut self.wanted);
        let (lock, wake) = &*self.shared;
        let mut queue = lock.lock().expect("queue lock");
        queue.replace(wanted);
        if !queue.wanted.is_empty() {
            wake.notify_all();
        }
    }

    /// Something asked for is still being decoded.
    pub fn busy(&self) -> bool {
        let queue = self.shared.0.lock().expect("queue lock");
        !queue.wanted.is_empty() || !queue.in_flight.is_empty()
    }
}

impl Drop for Thumbs {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        if let Ok(mut queue) = lock.lock() {
            queue.closed = true;
        }
        wake.notify_all();
    }
}

fn worker(
    shared: &Shared,
    tx: &Sender<(Picture, Slot, usize)>,
    ctx: &egui::Context,
    source: &ImageSource,
) {
    let (lock, wake) = &**shared;
    loop {
        let picture = {
            let mut queue = lock.lock().expect("queue lock");
            loop {
                if queue.closed {
                    return;
                }
                if let Some(next) = queue.take() {
                    break next;
                }
                queue = wake.wait(queue).expect("queue lock");
            }
        };
        let (slot, weight) = match render(source, &picture) {
            Ok(rendered) => {
                let image = &rendered.image;
                let size = [image.width as usize, image.height as usize];
                let texture = ctx.load_texture(
                    format!("{picture:?}"),
                    ColorImage::from_rgba_unmultiplied(size, &image.pixels),
                    TextureOptions::LINEAR,
                );
                let slot = Slot::Ready {
                    texture,
                    facts: rendered.facts,
                };
                (slot, image.pixels.len())
            }
            Err(reason) => (Slot::Failed(reason), 64),
        };
        let sent = tx.send((picture.clone(), slot, weight)).is_ok();
        lock.lock().expect("queue lock").in_flight.remove(&picture);
        if !sent {
            return;
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(path: &str) -> Picture {
        Picture::Game {
            path: path.into(),
            side: 96,
        }
    }

    #[test]
    fn lru_drops_the_least_recently_used_past_its_budget() {
        let mut lru = Lru::new(10);
        lru.insert("a", 1, 4);
        lru.insert("b", 2, 4);
        assert_eq!(lru.get(&"a"), Some(&1));
        lru.insert("c", 3, 4);
        assert!(lru.contains(&"a") && lru.contains(&"c"));
        assert!(!lru.contains(&"b"), "b was used longest ago");
        assert_eq!(lru.weight(), 8);
        lru.insert("a", 9, 2);
        assert_eq!((lru.weight(), lru.len()), (6, 2));
        lru.insert("huge", 0, 50);
        assert_eq!(lru.len(), 1, "a lone oversized entry stays");
    }

    #[test]
    fn the_queue_holds_only_this_frames_wishes_not_already_started() {
        let mut queue = Queue::default();
        queue.replace(vec![game("a"), game("b"), game("c")]);
        assert_eq!(queue.take(), Some(game("a")));
        queue.replace(vec![game("c"), game("a"), game("d")]);
        assert_eq!(
            queue.wanted.iter().cloned().collect::<Vec<_>>(),
            [game("c"), game("d")],
            "b scrolled away; a is in flight"
        );
    }
}
