//! Pruebas con documentos reales. Se indican por variable de entorno, por ejemplo:
//! LECTOR_EPUB=libro.epub LECTOR_DOCX=doc.docx LECTOR_PDF=doc.pdf cargo test -- --nocapture

use super::flow::{FlowDoc, LayoutParams};
use super::*;
use crate::search;
use crate::store::FontKind;
use eframe::egui;
use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn paginate(fd: &mut FlowDoc, width: f32, height: f32, size: f32) {
    let ctx = egui::Context::default();
    let p = LayoutParams { width, height, font_size: size, line_spacing: 1.5, font: FontKind::Serif, justify: true };
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| fd.relayout(ui.ctx(), p));
}

fn check_flow(mut fd: FlowDoc) {
    println!("título: {}", fd.title);
    println!("bloques: {}, índice: {}", fd.blocks.len(), fd.toc.len());
    for e in fd.toc.iter().take(8) {
        println!("  {}{} -> bloque {}", "  ".repeat(e.depth), e.title, e.unit);
    }
    for b in fd.blocks.iter().take(6) {
        println!("  [{:?}] {}", b.kind, b.text.chars().take(90).collect::<String>());
    }
    assert!(!fd.blocks.is_empty());

    paginate(&mut fd, 640.0, 760.0, 19.0);
    let big = fd.page_count();
    paginate(&mut fd, 640.0, 760.0, 26.0);
    let bigger = fd.page_count();
    println!("páginas: {big} (letra 19) / {bigger} (letra 26)");
    assert!(bigger > big, "con letra más grande debe haber más páginas");

    // Ida y vuelta posición <-> página en todas las páginas.
    for p in 0..fd.page_count() {
        let loc = fd.page_start(p);
        assert_eq!(fd.page_of(loc), p, "page_start/page_of no coinciden en la página {p}");
    }
    // Las páginas avanzan en orden por el documento.
    let mut last = 0;
    for p in 0..fd.page_count() {
        let r = fd.page_blocks(p);
        assert!(r.start >= last, "la página {p} retrocede");
        last = r.start;
    }
    let mid = fd.blocks.len() / 2;
    let hay = &fd.folded()[mid];
    if hay.len() > 12 {
        let needle: Vec<char> = hay[3..10].to_vec();
        assert!(!search::find_all(hay, &needle).is_empty());
    }
}

#[test]
fn epub_real() {
    let Some(p) = env_path("LECTOR_EPUB") else { return };
    check_flow(epub::load(&p).expect("EPUB"));
}

#[test]
fn docx_real() {
    let Some(p) = env_path("LECTOR_DOCX") else { return };
    check_flow(docx::load(&p).expect("DOCX"));
}

#[test]
fn pdf_real() {
    let Some(p) = env_path("LECTOR_PDF") else { return };
    // pdfium.dll debe estar junto al ejecutable de pruebas (target/.../deps).
    let pdfium = pdf::bind_pdfium().expect("pdfium.dll");
    let mut d = pdf::PdfDoc::load(pdfium, &p).expect("PDF");
    println!("título: {}, páginas: {}, índice: {}", d.title, d.page_count(), d.toc.len());
    for e in d.toc.iter().take(8) {
        println!("  {}{} -> pág {}", "  ".repeat(e.depth), e.title, e.unit + 1);
    }
    let page = (d.page_count() / 2).min(37);
    let t = d.text(page);
    println!("pág {}: {} caracteres: {}", page + 1, t.chars.len(), t.slice(0, 120));
    assert_eq!(t.chars.len(), t.rects.len());
    let first = t.rects.iter().position(|r| r.is_positive()).expect("texto con posición");
    let c = t.rects[first].center();
    assert!(t.hit(c).is_some_and(|i| i == first || i == first + 1));
    for (i, (c, r)) in t.chars.iter().zip(&t.rects).enumerate().skip(40).take(6) {
        println!("  carácter {i} {c:?} -> {r:?}");
    }
    println!("tamaño de página: {:?}", d.sizes[page]);
    let rects = t.range_rects(0, t.chars.len().min(200));
    println!("líneas en los primeros 200 caracteres: {}", rects.len());
    assert!(!rects.is_empty());
}
