//! Gestion des couleurs (profils ICC) : conversion des images vers le profil de sortie
//! et épreuve écran.

use image::RgbaImage;
// Moteur : LittleCMS 2 (le même calcul que la plupart des logiciels, proche de Photoshop).

use lcms2::{ColorSpaceSignature, Flags, PixelFormat, Profile as LProfile, Transform};
use std::path::Path;
use std::sync::Arc;

type Px = [u8; 4];

fn transform(from: &LProfile, to: &LProfile, intent: Intent, bpc: bool) -> Result<Transform<Px, Px>, String> {
    let flags = if bpc { Flags::BLACKPOINT_COMPENSATION } else { Flags::default() };
    Transform::new_flags(from, PixelFormat::RGBA_8, to, PixelFormat::RGBA_8, intent.lcms(), flags).map_err(|e| e.to_string())
}

#[derive(Clone)]
pub struct Profile {
    pub name: String,
    pub path: std::path::PathBuf,
    pub bytes: Arc<Vec<u8>>,
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let p = LProfile::new_icc(&bytes).map_err(|e| format!("profil illisible : {e}"))?;
        if p.color_space() != ColorSpaceSignature::RgbData {
            return Err("ce profil n'est pas RVB (CMJN ou gris) : choisis un profil RVB".into());
        }
        let srgb = LProfile::new_srgb();
        transform(&srgb, &p, Intent::Relative, false)
            .and_then(|_| transform(&p, &srgb, Intent::Relative, false))
            .map_err(|e| format!("profil non utilisable par le logiciel ({e})"))?;
        Ok(Self {
            path: path.to_path_buf(),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "profil".into()),
            bytes: Arc::new(bytes),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Intent {
    Perceptual,
    /// Réglage par défaut de Photoshop.
    #[default]
    Relative,
    Saturation,
    Absolute,
}

impl Intent {
    pub const ALL: [Intent; 4] = [Intent::Perceptual, Intent::Relative, Intent::Saturation, Intent::Absolute];
    pub fn label(self) -> &'static str {
        match self {
            Intent::Perceptual => "Perceptuel (photos)",
            Intent::Relative => "Colorimétrique relatif",
            Intent::Saturation => "Saturation (aplats)",
            Intent::Absolute => "Colorimétrique absolu",
        }
    }
    fn lcms(self) -> lcms2::Intent {
        match self {
            Intent::Perceptual => lcms2::Intent::Perceptual,
            Intent::Relative => lcms2::Intent::RelativeColorimetric,
            Intent::Saturation => lcms2::Intent::Saturation,
            Intent::Absolute => lcms2::Intent::AbsoluteColorimetric,
        }
    }
}

#[derive(Clone, Default)]
pub struct Color {
    /// Profil des images (sRGB si vide).
    pub src: Option<Profile>,
    /// Profil de sortie : imprimante + papier (sRGB si vide).
    pub out: Option<Profile>,
    pub intent: Intent,
    /// Aperçu écran qui simule le rendu du profil de sortie.
    pub softproof: bool,
    /// Compensation du point noir (cochée par défaut dans Photoshop).
    pub bpc: bool,
    /// Correction de teinte de toute la page, en % par canal (rouge, vert, bleu).
    pub gain: [i32; 3],
    /// Luminosité et contraste, en % (−50 à +50).
    pub brightness: i32,
    pub contrast: i32,
}

fn parse(p: &Option<Profile>) -> Result<LProfile, String> {
    match p {
        Some(p) => LProfile::new_icc(&p.bytes).map_err(|e| e.to_string()),
        None => Ok(LProfile::new_srgb()),
    }
}

fn convert(img: &RgbaImage, from: &LProfile, to: &LProfile, intent: Intent, bpc: bool) -> Result<RgbaImage, String> {
    let t = transform(from, to, intent, bpc)?;
    let src: Vec<Px> = img.pixels().map(|p| p.0).collect();
    let mut dst = vec![[0u8; 4]; src.len()];
    t.transform_pixels(&src, &mut dst);
    let mut out = RgbaImage::new(img.width(), img.height());
    // La transparence est conservée telle quelle.
    for ((o, d), s) in out.pixels_mut().zip(&dst).zip(&src) {
        o.0 = [d[0], d[1], d[2], s[3]];
    }
    Ok(out)
}

impl Color {
    /// Applique la correction de teinte (avant toute conversion ICC).
    fn tint(&self, img: &RgbaImage) -> RgbaImage {
        let mut out = img.clone();
        if self.gain == [0; 3] && self.brightness == 0 && self.contrast == 0 {
            return out;
        }
        let (b, ct) = (self.brightness as f32 / 100.0, 1.0 + self.contrast as f32 / 100.0);
        let lut: Vec<[u8; 256]> = self
            .gain
            .iter()
            .map(|g| {
                let k = 1.0 + *g as f32 / 100.0;
                let mut t = [0u8; 256];
                for (i, v) in t.iter_mut().enumerate() {
                    let x = (i as f32 / 255.0 * k).clamp(0.0, 1.0);
                    *v = (((x - 0.5) * ct + 0.5 + b) * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                t
            })
            .collect();
        for p in out.pixels_mut() {
            for c in 0..3 {
                p[c] = lut[c][p[c] as usize];
            }
        }
        out
    }

    pub fn signature(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{:?}|{}|{}",
            self.src.as_ref().map_or("", |p| &p.name),
            self.out.as_ref().map_or("", |p| &p.name),
            self.intent as u8,
            self.bpc,
            self.softproof && self.out.is_some(),
            self.gain,
            self.brightness,
            self.contrast
        )
    }

    /// Image prête pour le PDF / l'imprimante : profil des images → profil de sortie.
    pub fn for_output(&self, img: &RgbaImage) -> Result<RgbaImage, String> {
        let img = &self.tint(img);
        if self.src.is_none() && self.out.is_none() {
            return Ok(img.clone());
        }
        convert(img, &parse(&self.src)?, &parse(&self.out)?, self.intent, self.bpc)
    }

    /// Image telle qu'elle doit s'afficher à l'écran (sRGB).
    pub fn for_screen(&self, img: &RgbaImage) -> RgbaImage {
        let img = &self.tint(img);
        let srgb = LProfile::new_srgb();
        let attempt = || -> Result<RgbaImage, String> {
            let src = parse(&self.src)?;
            if self.softproof && self.out.is_some() {
                let out = parse(&self.out)?;
                let printed = convert(img, &src, &out, self.intent, self.bpc)?;
                convert(&printed, &out, &srgb, Intent::Relative, false)
            } else if self.src.is_some() {
                convert(img, &src, &srgb, Intent::Relative, false)
            } else {
                Ok(img.clone())
            }
        };
        attempt().unwrap_or_else(|_| img.clone())
    }
}

/// Dossiers des profils ICC installés sur le système.
fn profile_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if cfg!(windows) {
        let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        vec![Path::new(&win).join("System32").join("spool").join("drivers").join("color")]
    } else if cfg!(target_os = "macos") {
        let mut v = vec!["/Library/ColorSync/Profiles".into(), "/System/Library/ColorSync/Profiles".into()];
        v.extend(home.map(|h| h.join("Library/ColorSync/Profiles")));
        v
    } else {
        let mut v = vec!["/usr/share/color/icc".into(), "/usr/local/share/color/icc".into(), "/var/lib/color/icc".into()];
        if let Some(h) = home {
            v.push(h.join(".local/share/icc"));
            v.push(h.join(".color/icc"));
        }
        v
    }
}

/// Profils installés sur le système (un niveau de sous-dossiers, comme /usr/share/color/icc/colord).
pub fn installed_profiles() -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for dir in profile_dirs() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                files.extend(std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()));
            } else {
                files.push(p);
            }
        }
    }
    let mut v: Vec<_> = files
        .into_iter()
        .filter(|p| {
            p.extension()
                .map(|e| {
                    let e = e.to_string_lossy().to_lowercase();
                    e == "icc" || e == "icm"
                })
                .unwrap_or(false)
        })
        .collect();
    v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn convert_with_installed_profiles() {
        let list = installed_profiles();
        println!("{} profils installés", list.len());
        let img = RgbaImage::from_pixel(8, 8, image::Rgba([200, 80, 40, 255]));
        let mut ok = 0;
        for p in &list {
            let Ok(pr) = Profile::load(p) else { continue };
            let c = Color { out: Some(pr.clone()), softproof: true, ..Default::default() };
            match c.for_output(&img) {
                Ok(o) => { ok += 1; println!("{} -> {:?}", pr.name, o.get_pixel(0, 0)); }
                Err(e) => println!("{} : {e}", pr.name),
            }
        }
        assert!(ok > 0 || list.is_empty());
    }
}



