//! Réglages enregistrés automatiquement (et dernière planche), dans
//! %APPDATA%\PlancheContact\reglages.json (à côté de l'exe en version portable).

use crate::icc::Intent;
use crate::model::{Format, Settings, Shape};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct SavedDesign {
    pub path: PathBuf,
    /// Page du PDF (0 pour une image).
    #[serde(default)]
    pub page: u32,
    pub count: u32,
    pub width_mm: f32,
    pub rotated: bool,
    /// Format fixe (largeur / hauteur), image recadrée.
    #[serde(default)]
    pub frame: Option<f32>,
    #[serde(default = "center")]
    pub pan: [f32; 2],
}

fn center() -> [f32; 2] {
    [0.5, 0.5]
}

/// Planche enregistrée sous un nom : designs + mise en page.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedPlanche {
    pub margin: f32,
    pub gap: f32,
    pub pages: usize,
    pub shape: Shape,
    pub show_contour: bool,
    pub magnet: bool,
    pub bleed: f32,
    pub mirror: bool,
    #[serde(default)]
    pub corners: bool,
    pub designs: Vec<SavedDesign>,
}

impl SavedPlanche {
    pub fn settings(&self) -> Settings {
        Settings {
            margin: self.margin,
            gap: self.gap,
            pages: self.pages.max(1),
            shape: self.shape,
            show_contour: self.show_contour,
            magnet: self.magnet,
            bleed: self.bleed,
            mirror: self.mirror,
            corners: self.corners,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Prefs {
    pub margin: f32,
    pub gap: f32,
    pub pages: usize,
    pub shape: Shape,
    pub show_contour: bool,
    pub magnet: bool,
    pub bleed: f32,
    pub mirror: bool,
    pub corners: bool,
    pub import_mm: f32,
    pub zoom: f32,
    pub printer: String,
    pub src_icc: Option<PathBuf>,
    pub out_icc: Option<PathBuf>,
    pub intent: Intent,
    pub softproof: bool,
    pub bpc: bool,
    pub gain: [i32; 3],
    pub brightness: i32,
    pub contrast: i32,
    pub designs: Vec<SavedDesign>,
    /// Formats proposés au recadrage (magnet, mug…).
    pub formats: Vec<Format>,
    /// Format appliqué aux nouvelles images (nom), ou aucun.
    pub import_format: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        let s = Settings::default();
        Self {
            margin: s.margin,
            gap: s.gap,
            pages: s.pages,
            shape: s.shape,
            show_contour: s.show_contour,
            magnet: s.magnet,
            bleed: s.bleed,
            mirror: s.mirror,
            corners: s.corners,
            import_mm: 75.0,
            zoom: 1.0,
            printer: String::new(),
            src_icc: None,
            out_icc: None,
            intent: Intent::default(),
            softproof: false,
            bpc: true,
            gain: [0; 3],
            brightness: 0,
            contrast: 0,
            designs: Vec::new(),
            formats: Format::defaults(),
            import_format: None,
        }
    }
}

/// Dossier des réglages. Version portable : à côté de l'exe (dossier « PlancheContact-donnees »).
pub fn dir() -> PathBuf {
    if cfg!(feature = "portable") {
        if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("PlancheContact-donnees"))) {
            return d;
        }
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("PlancheContact")
}

fn file() -> PathBuf {
    dir().join("reglages.json")
}

fn planches_dir() -> PathBuf {
    dir().join("planches")
}

/// Nom de fichier sans caractères interdits par Windows.
fn safe(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect()
}

/// Noms des planches enregistrées, triés.
pub fn list_planches() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(planches_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

pub fn save_planche(name: &str, p: &SavedPlanche) -> Result<String, String> {
    let name = safe(name);
    if name.is_empty() {
        return Err("donne un nom à la planche".into());
    }
    std::fs::create_dir_all(planches_dir()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(planches_dir().join(format!("{name}.json")), json).map_err(|e| e.to_string())?;
    Ok(name)
}

pub fn load_planche(name: &str) -> Result<SavedPlanche, String> {
    let s = std::fs::read_to_string(planches_dir().join(format!("{}.json", safe(name)))).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

pub fn delete_planche(name: &str) -> Result<(), String> {
    std::fs::remove_file(planches_dir().join(format!("{}.json", safe(name)))).map_err(|e| e.to_string())
}

pub fn load() -> Prefs {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(json: &str) {
    let f = file();
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Écriture dans un fichier temporaire puis renommage : jamais de fichier à moitié écrit.
    let tmp = f.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &f);
    }
}
