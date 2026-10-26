use eframe::egui;
use pdfcraft_render::{RenderConfig, RenderPool, RenderRequest};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub struct Frame {
    pub texture: egui::TextureHandle,
    pub scale: f32,
    pub generation: u64,
}
#[derive(Default)]
pub struct Raster {
    pub frames: HashMap<usize, Frame>,
    pub thumbs: HashMap<usize, egui::TextureHandle>,
    pool: Option<RenderPool>,
    generation: u64,
    pending: HashSet<(usize, u32, bool)>,
}
impl Raster {
    pub fn document(&mut self, bytes: Arc<Vec<u8>>, reset: bool, changed: Option<usize>) {
        self.generation = self.generation.wrapping_add(1);
        if reset {
            self.frames.clear();
            self.thumbs.clear();
        } else {
            // Unchanged page textures stay current; the changed page remains visible
            // while its replacement is rendered. Never flash a blank/loading page.
            for (page, frame) in &mut self.frames {
                if changed.is_some() && Some(*page) != changed {
                    frame.generation = self.generation;
                }
            }
            if let Some(page) = changed {
                self.thumbs.remove(&page);
            } else {
                self.thumbs.clear();
            }
        }
        self.pending.clear();
        self.pool = Some(RenderPool::new(bytes, 2, RenderConfig::default()));
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(pool) = &self.pool else {
            return errors;
        };
        while let Some(out) = pool.try_recv() {
            let thumb = out.request.tag & 1 == 1;
            self.pending
                .remove(&(out.request.page, out.request.scale.to_bits(), thumb));
            if out.request.tag >> 1 != self.generation {
                continue;
            }
            if let Some(error) = out.error {
                errors.push(error);
                continue;
            }
            if !out.warnings.is_empty() {
                errors.push("Une partie du contenu de cette page n’a pas pu être rendue.".into());
            }
            let texture = ctx.load_texture(
                format!("page-{}-{}", out.request.page, out.request.tag),
                egui::ColorImage::from_rgba_premultiplied(
                    [out.width as usize, out.height as usize],
                    &out.rgba,
                ),
                egui::TextureOptions::LINEAR,
            );
            if thumb {
                self.thumbs.insert(out.request.page, texture);
            } else {
                self.frames.insert(
                    out.request.page,
                    Frame {
                        texture,
                        scale: out.request.scale,
                        generation: self.generation,
                    },
                );
            }
        }
        errors
    }
    pub fn current(&self, page: usize) -> bool {
        self.frames
            .get(&page)
            .is_some_and(|f| f.generation == self.generation)
    }
    pub fn queue(&mut self, visible: &[usize], thumbs: &[usize], scale: f32, ctx: &egui::Context) {
        let Some(pool) = &self.pool else { return };
        let mut requests = Vec::new();
        for &page in visible {
            if self
                .frames
                .get(&page)
                .is_none_or(|f| f.generation != self.generation || (f.scale - scale).abs() > 0.05)
            {
                requests.push(RenderRequest {
                    page,
                    scale,
                    tag: self.generation << 1,
                    ..Default::default()
                });
            }
        }
        for &page in thumbs {
            if !self.thumbs.contains_key(&page) {
                requests.push(RenderRequest {
                    page,
                    scale: 0.4,
                    tag: (self.generation << 1) | 1,
                    preview: true,
                    ..Default::default()
                });
            }
        }
        self.pending = requests
            .iter()
            .map(|r| (r.page, r.scale.to_bits(), r.tag & 1 == 1))
            .collect();
        pool.set_queue(requests);
        if !self.pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        // Keep a bounded set of full-resolution textures; thumbnails are much smaller.
        if self.frames.len() > 12 {
            self.frames.retain(|page, _| visible.contains(page));
        }
        if self.thumbs.len() > 96 {
            self.thumbs.retain(|page, _| thumbs.contains(page));
        }
    }
}

pub struct Background {
    pool: RenderPool,
    pub texture: Option<egui::TextureHandle>,
    page: usize,
}
impl Background {
    pub fn new(bytes: Arc<Vec<u8>>, page: usize, scale: f32) -> Self {
        let pool = RenderPool::new(bytes, 1, RenderConfig::default());
        pool.set_queue(vec![RenderRequest {
            page,
            scale,
            ..Default::default()
        }]);
        Self {
            pool,
            texture: None,
            page,
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<String> {
        if let Some(out) = self.pool.try_recv() {
            if let Some(error) = out.error {
                return Some(error);
            }
            self.texture = Some(ctx.load_texture(
                format!("source-free-{}", self.page),
                egui::ColorImage::from_rgba_premultiplied(
                    [out.width as usize, out.height as usize],
                    &out.rgba,
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
        if self.texture.is_none() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        None
    }
}
