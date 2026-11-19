use crate::{ArtError, FrameSource, Image, Raster, Theme};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    id: String,
    theme: Theme,
    frame: u32,
    size_px: u32,
}

#[derive(Debug)]
pub struct ArtCache {
    entries: Mutex<LruStore>,
}

#[derive(Debug)]
struct LruStore {
    capacity: usize,
    values: HashMap<CacheKey, Arc<Image>>,
    order: VecDeque<CacheKey>,
}

impl ArtCache {
    pub fn new() -> Self {
        Self::with_capacity(48)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(LruStore {
                capacity,
                values: HashMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    pub fn default_capacity() -> usize {
        48
    }

    pub fn get(
        &self,
        source: &dyn FrameSource,
        id: &str,
        theme: Theme,
        frame: u32,
        size_px: u32,
    ) -> Option<Result<Arc<Image>, ArtError>> {
        let key = CacheKey {
            id: id.to_owned(),
            theme,
            frame,
            size_px,
        };

        let mut store = match self.entries.lock() {
            Ok(store) => store,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(image) = store.values.get(&key).cloned() {
            store.order.retain(|item| item != &key);
            store.order.push_back(key);
            return Some(Ok(image));
        }

        let svg = source.svg(id, theme, frame)?;
        let image = match Raster::render(&svg, size_px) {
            Ok(image) => Arc::new(image),
            Err(error) => return Some(Err(error)),
        };
        if store.capacity == 0 {
            return Some(Ok(image));
        }
        if store.values.len() >= store.capacity
            && let Some(oldest) = store.order.pop_front()
        {
            store.values.remove(&oldest);
        }
        store.values.insert(key.clone(), Arc::clone(&image));
        store.order.push_back(key);
        Some(Ok(image))
    }

    pub fn clear(&self) {
        match self.entries.lock() {
            Ok(mut store) => {
                store.values.clear();
                store.order.clear();
            }
            Err(poisoned) => {
                let mut store = poisoned.into_inner();
                store.values.clear();
                store.order.clear();
            }
        }
    }

    pub fn bytes_used(&self) -> usize {
        match self.entries.lock() {
            Ok(store) => store.values.values().map(|image| image.pixels.len()).sum(),
            Err(poisoned) => poisoned
                .into_inner()
                .values
                .values()
                .map(|image| image.pixels.len())
                .sum(),
        }
    }
}

impl Default for ArtCache {
    fn default() -> Self {
        Self::new()
    }
}
