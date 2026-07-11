use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use image::DynamicImage;

#[derive(Debug)]
pub struct ImageLoader {
    cache: Arc<Mutex<std::collections::HashMap<PathBuf, DynamicImage>>>,
}

impl ImageLoader {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn load(&self, path: &PathBuf) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        let mut cache = self.cache.lock().unwrap();
        
        if let Some(img) = cache.get(path) {
            return Ok(img.clone());
        }

        let img = image::open(path)?;
        cache.insert(path.clone(), img.clone());
        Ok(img)
    }

    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    pub fn cache_size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}

impl Default for ImageLoader {
    fn default() -> Self {
        Self::new()
    }
}
