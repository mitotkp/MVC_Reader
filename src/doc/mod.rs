pub mod docx;
pub mod epub;
pub mod flow;
pub mod pdf;
pub mod xml;

use crate::store::Loc;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct TocEntry {
    pub title: String,
    pub depth: usize,
    /// Página (PDF) o bloque (EPUB/DOCX).
    pub unit: usize,
}

/// Motivo por el que no se pudo abrir un documento (el texto lo pone `i18n`).
#[derive(Debug)]
pub enum LoadError {
    Open(String),
    Damaged(String),
    NoContainer,
    NoOpf,
    NoText,
    NotWord,
    Password,
    Pdf(String),
    NoPages,
    NoPdfium(String),
}

pub enum Document {
    Pdf(pdf::PdfDoc),
    Flow(flow::FlowDoc),
}

pub fn kind_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => Some("PDF"),
        "epub" => Some("EPUB"),
        "docx" => Some("DOCX"),
        _ => None,
    }
}

impl Document {
    pub fn title(&self) -> &str {
        match self {
            Document::Pdf(d) => &d.title,
            Document::Flow(d) => &d.title,
        }
    }

    pub fn toc(&self) -> &[TocEntry] {
        match self {
            Document::Pdf(d) => &d.toc,
            Document::Flow(d) => &d.toc,
        }
    }

    pub fn page_count(&self) -> usize {
        match self {
            Document::Pdf(d) => d.page_count(),
            Document::Flow(d) => d.page_count(),
        }
    }

    pub fn page_of(&self, loc: Loc) -> usize {
        match self {
            Document::Pdf(_) => loc.unit,
            Document::Flow(d) => d.page_of(loc),
        }
    }

    pub fn page_start(&self, page: usize) -> Loc {
        match self {
            Document::Pdf(_) => Loc::new(page, 0),
            Document::Flow(d) => d.page_start(page),
        }
    }

    /// Página de una entrada del índice.
    pub fn unit_page(&self, unit: usize) -> usize {
        self.page_of(Loc::new(unit, 0))
    }

    /// Entrada del índice que corresponde a una página: la de mayor página que no la supere.
    /// (El índice de algunos PDF no está en orden de páginas.)
    pub fn toc_current(&self, page: usize) -> Option<usize> {
        // En texto fluido vale el capítulo en curso al inicio de la página,
        // no el último que empieza en ella.
        if let Document::Flow(f) = self {
            let top = f.page_start(page).unit;
            return f.toc.iter().enumerate().filter(|(_, e)| e.unit <= top).max_by_key(|(i, e)| (e.unit, *i)).map(|(i, _)| i);
        }
        let mut best: Option<(usize, usize)> = None;
        for (i, e) in self.toc().iter().enumerate() {
            let p = self.unit_page(e.unit);
            if p <= page && best.is_none_or(|(bp, _)| p >= bp) {
                best = Some((p, i));
            }
        }
        best.map(|(_, i)| i)
    }

    pub fn is_pdf(&self) -> bool {
        matches!(self, Document::Pdf(_))
    }
}

#[cfg(test)]
mod tests;
