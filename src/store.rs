//! Persistencia en disco: preferencias, recientes y datos por documento
//! (posición, marcadores, subrayados y notas). Todo se guarda como JSON en
//! %APPDATA%\MVCreader.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Posición dentro de un documento. `unit` es la página en PDF y el bloque
/// (párrafo) en EPUB/DOCX; `ch` es el índice de carácter dentro de esa unidad.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Loc {
    pub unit: usize,
    pub ch: usize,
}

impl Loc {
    pub fn new(unit: usize, ch: usize) -> Self {
        Self { unit, ch }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bookmark {
    pub loc: Loc,
    pub label: String,
    pub created: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Highlight {
    pub id: u64,
    pub start: Loc,
    /// Exclusivo.
    pub end: Loc,
    pub color: usize,
    pub text: String,
    #[serde(default)]
    pub note: String,
    pub created: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub id: u64,
    /// `None` = nota general del documento.
    pub loc: Option<Loc>,
    pub text: String,
    pub created: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DocData {
    pub path: String,
    pub title: String,
    pub position: Loc,
    #[serde(default)]
    pub total_pages: usize,
    #[serde(default)]
    pub current_page: usize,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default)]
    pub highlights: Vec<Highlight>,
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Zoom propio del PDF (1.0 = 100 %).
    #[serde(default)]
    pub pdf_zoom: Option<f32>,
    #[serde(default)]
    pub pdf_fit: Option<PdfFit>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeKind {
    #[default]
    Light,
    Sepia,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PdfFit {
    #[default]
    Width,
    Page,
    Free,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontKind {
    #[default]
    Serif,
    Sans,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecentItem {
    pub key: String,
    pub path: String,
    pub title: String,
    pub kind: String,
    pub progress: f32,
    pub last_opened: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeKind,
    pub font_size: f32,
    pub font: FontKind,
    pub line_spacing: f32,
    /// Ancho máximo de la columna de texto en EPUB/DOCX (px lógicos).
    pub text_width: f32,
    pub justify: bool,
    pub pdf_invert_dark: bool,
    pub ui_scale: f32,
    pub sidebar_open: bool,
    /// `None` = todavía no elegido: se usa el idioma de Windows.
    pub language: Option<crate::i18n::Lang>,
    pub recent: Vec<RecentItem>,
    pub window_size: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeKind::Light,
            font_size: 19.0,
            font: FontKind::Serif,
            line_spacing: 1.5,
            text_width: 760.0,
            justify: true,
            pdf_invert_dark: true,
            ui_scale: 1.0,
            sidebar_open: true,
            language: None,
            recent: Vec::new(),
            window_size: None,
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Identificador único y creciente para subrayados y notas.
pub fn new_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    base.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Carpeta de datos: `MVCREADER_DATA_DIR` si está definida; si junto al ejecutable hay
/// una carpeta `datos` (modo portable), esa; si no, %APPDATA%\MVCreader.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("MVCREADER_DATA_DIR") {
        return PathBuf::from(d);
    }
    if let Some(portable) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("datos"))) {
        if portable.is_dir() {
            return portable;
        }
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("MVCreader");
    // La primera versión se llamaba "Lector": se trae su carpeta para conservar las notas.
    let old = base.join("LectorDocs");
    if !dir.exists() && old.is_dir() {
        let _ = fs::rename(&old, &dir);
    }
    dir
}

fn docs_dir() -> PathBuf {
    data_dir().join("docs")
}

fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

/// Lee un JSON aceptando BOM (lo agregan algunos editores al guardar). Si el archivo
/// existe pero no se puede leer, se guarda una copia `.bak` antes de que se sobrescriba.
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str(text.trim_start_matches('\u{FEFF}')) {
        Ok(v) => Some(v),
        Err(_) => {
            let _ = fs::copy(path, path.with_extension("json.bak"));
            None
        }
    }
}

pub fn load_settings() -> Settings {
    read_json(&data_dir().join("settings.json")).unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = write_atomic(&data_dir().join("settings.json"), &json);
    }
}

pub fn load_doc_data(key: &str) -> Option<DocData> {
    read_json(&docs_dir().join(format!("{key}.json")))
}

pub fn save_doc_data(key: &str, d: &DocData) {
    if let Ok(json) = serde_json::to_string_pretty(d) {
        let _ = write_atomic(&docs_dir().join(format!("{key}.json")), &json);
    }
}

/// Huella del archivo (tamaño + primeros y últimos 64 KB, FNV-1a de 64 bits).
/// Así los datos sobreviven aunque el archivo se mueva o se renombre.
pub fn file_key(path: &Path) -> std::io::Result<String> {
    const CHUNK: u64 = 64 * 1024;
    let mut f = fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    feed(&len.to_le_bytes());
    let mut buf = vec![0u8; CHUNK as usize];
    let n = f.read(&mut buf)?;
    feed(&buf[..n]);
    if len > CHUNK * 2 {
        f.seek(SeekFrom::Start(len - CHUNK))?;
        let n = f.read(&mut buf)?;
        feed(&buf[..n]);
    }
    Ok(format!("{hash:016x}"))
}
