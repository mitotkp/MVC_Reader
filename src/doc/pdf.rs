//! Documentos PDF mediante PDFium (pdfium.dll junto al ejecutable).
//! Solo se renderizan las páginas visibles y se guardan pocas en caché.

use eframe::egui::{self, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::{LoadError, TocEntry};

pub struct PageText {
    pub chars: Vec<char>,
    /// Rectángulo de cada carácter en puntos, origen arriba a la izquierda.
    pub rects: Vec<Rect>,
}

pub struct PdfDoc {
    doc: PdfDocument<'static>,
    pub title: String,
    pub toc: Vec<TocEntry>,
    /// Tamaño de cada página en puntos.
    pub sizes: Vec<Vec2>,
    textures: HashMap<usize, (TextureHandle, i32, bool)>,
    texture_order: Vec<usize>,
    texts: HashMap<usize, Arc<PageText>>,
    text_order: Vec<usize>,
}

/// Carga pdfium.dll una sola vez por proceso (PDFium no admite inicializarse dos veces).
pub fn bind_pdfium() -> Result<&'static Pdfium, LoadError> {
    static PDFIUM: std::sync::OnceLock<&'static Pdfium> = std::sync::OnceLock::new();
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let p = load_pdfium()?;
    Ok(PDFIUM.get_or_init(|| p))
}

fn load_pdfium() -> Result<&'static Pdfium, LoadError> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&exe_dir))
        .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")))
        .map_err(|e| LoadError::NoPdfium(format!("{e:?}")))?;
    Ok(Box::leak(Box::new(Pdfium::new(bindings))))
}

impl PdfDoc {
    pub fn load(pdfium: &'static Pdfium, path: &Path) -> Result<Self, LoadError> {
        let doc = pdfium.load_pdf_from_file(path, None).map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => LoadError::Password,
            e => LoadError::Pdf(format!("{e:?}")),
        })?;
        let sizes: Vec<Vec2> = doc
            .pages()
            .page_sizes()
            .map_err(|e| LoadError::Pdf(format!("{e:?}")))?
            .iter()
            .map(|r| vec2(r.width().value.abs().max(1.0), r.height().value.abs().max(1.0)))
            .collect();
        if sizes.is_empty() {
            return Err(LoadError::NoPages);
        }
        let title = doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());

        let mut toc = Vec::new();
        for bm in doc.bookmarks().iter().take(5000) {
            let Some(title) = bm.title() else { continue };
            let Some(page) = bm.destination().and_then(|d| d.page_index().ok()) else { continue };
            let mut depth: usize = 0;
            let mut p = bm.parent();
            while let Some(pp) = p {
                depth += 1;
                p = pp.parent();
                if depth > 10 {
                    break;
                }
            }
            toc.push(TocEntry { title: title.trim().to_string(), depth: depth.saturating_sub(1), unit: page.max(0) as usize });
        }

        Ok(Self {
            doc,
            title,
            toc,
            sizes,
            textures: HashMap::new(),
            texture_order: Vec::new(),
            texts: HashMap::new(),
            text_order: Vec::new(),
        })
    }

    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    /// Textura de la página renderizada a `width_px` píxeles de ancho.
    pub fn texture(&mut self, ctx: &egui::Context, page: usize, width_px: i32, invert: bool) -> Option<TextureHandle> {
        let width_px = width_px.clamp(64, 4000);
        if let Some((t, w, inv)) = self.textures.get(&page) {
            // Se reutiliza si la diferencia de tamaño es pequeña (evita re-renderizar al redimensionar).
            if *inv == invert && (*w - width_px).abs() <= (width_px / 20).max(4) {
                return Some(t.clone());
            }
        }
        let pg = self.doc.pages().get(page as i32).ok()?;
        let cfg = PdfRenderConfig::new().set_target_width(width_px).render_form_data(true);
        let bmp = pg.render_with_config(&cfg).ok()?;
        let (w, h) = (bmp.width() as usize, bmp.height() as usize);
        let mut rgba = bmp.as_rgba_bytes();
        if invert {
            // Inversión suave: fondo gris oscuro en lugar de negro puro.
            for px in rgba.chunks_exact_mut(4) {
                for c in &mut px[..3] {
                    *c = 232 - (*c as u16 * 204 / 255) as u8;
                }
            }
        }
        let img = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        let tex = ctx.load_texture(format!("pdf-{page}"), img, TextureOptions::LINEAR);
        self.textures.insert(page, (tex.clone(), width_px, invert));
        self.texture_order.retain(|p| *p != page);
        self.texture_order.push(page);
        while self.texture_order.len() > 4 {
            let old = self.texture_order.remove(0);
            self.textures.remove(&old);
        }
        Some(tex)
    }

    pub fn clear_textures(&mut self) {
        self.textures.clear();
        self.texture_order.clear();
    }

    fn extract_text(&self, page: usize) -> PageText {
        let mut chars = Vec::new();
        let mut rects = Vec::new();
        let h = self.sizes.get(page).map_or(0.0, |s| s.y);
        if let Ok(pg) = self.doc.pages().get(page as i32) {
            // Las coordenadas del texto son relativas al MediaBox; lo visible es el CropBox,
            // que puede no empezar en (0, 0).
            let (left, top) = pg
                .boundaries()
                .crop()
                .or_else(|_| pg.boundaries().media())
                .map(|b| (b.bounds.left().value, b.bounds.top().value))
                .unwrap_or((0.0, h));
            if let Ok(text) = pg.text() {
                for c in text.chars().iter() {
                    let ch = c.unicode_char().unwrap_or(' ');
                    let r = match c.loose_bounds() {
                        Ok(b) if b.width().value > 0.0 && !matches!(ch, '\r' | '\n' | '\u{2}') => Rect::from_min_max(
                            pos2(b.left().value - left, top - b.top().value),
                            pos2(b.right().value - left, top - b.bottom().value),
                        ),
                        _ => Rect::NOTHING,
                    };
                    chars.push(ch);
                    rects.push(r);
                }
            }
        }
        PageText { chars, rects }
    }

    /// Texto de una página con caché (para dibujar subrayados y seleccionar).
    pub fn text(&mut self, page: usize) -> Arc<PageText> {
        if let Some(t) = self.texts.get(&page) {
            return t.clone();
        }
        let t = Arc::new(self.extract_text(page));
        self.texts.insert(page, t.clone());
        self.text_order.push(page);
        if self.text_order.len() > 8 {
            let old = self.text_order.remove(0);
            self.texts.remove(&old);
        }
        t
    }

    /// Texto sin pasar por la caché (para recorrer todo el documento al buscar).
    pub fn text_uncached(&self, page: usize) -> PageText {
        if let Some(t) = self.texts.get(&page) {
            return PageText { chars: t.chars.clone(), rects: Vec::new() };
        }
        self.extract_text(page)
    }
}

impl PageText {
    /// Carácter más cercano a un punto (en coordenadas de página).
    pub fn hit(&self, p: Pos2) -> Option<usize> {
        let mut best = None;
        let mut best_d = f32::INFINITY;
        for (i, r) in self.rects.iter().enumerate() {
            if !r.is_positive() {
                continue;
            }
            if r.contains(p) {
                return Some(if p.x > r.center().x { i + 1 } else { i });
            }
            let dx = (r.left() - p.x).max(p.x - r.right()).max(0.0);
            let dy = (r.top() - p.y).max(p.y - r.bottom()).max(0.0);
            let d = dx + dy * 4.0;
            if d < best_d {
                best_d = d;
                best = Some(if p.x > r.center().x { i + 1 } else { i });
            }
        }
        best.map(|i| i.min(self.chars.len()))
    }

    /// Rectángulos (por línea) que cubren `[s, e)`, en coordenadas de página.
    pub fn range_rects(&self, s: usize, e: usize) -> Vec<Rect> {
        let mut out: Vec<Rect> = Vec::new();
        for r in self.rects.iter().take(e.min(self.rects.len())).skip(s) {
            if !r.is_positive() {
                continue;
            }
            match out.last_mut() {
                Some(last)
                    if (last.center().y - r.center().y).abs() < last.height().min(r.height()) * 0.5
                        && r.left() >= last.left() - 2.0 =>
                {
                    *last = last.union(*r);
                }
                _ => out.push(*r),
            }
        }
        out
    }

    pub fn slice(&self, s: usize, e: usize) -> String {
        let s = s.min(self.chars.len());
        let e = e.min(self.chars.len());
        let mut out = String::new();
        for &c in &self.chars[s..e] {
            match c {
                '\r' => {}
                '\n' => out.push(' '),
                '\u{2}' | '\u{FFFE}' => {}
                c => out.push(c),
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
