//! Lecture des PDF : chaque page devient une image.
//! - Windows : moteur PDF intégré (Windows.Data.Pdf, aucune DLL à installer).
//! - Mac et Linux : outils de Poppler (`pdfinfo`, `pdftoppm`), à installer à part
//!   (Linux : paquet poppler-utils ; Mac : `brew install poppler`).

use image::RgbaImage;
use std::path::Path;

/// Côté le plus long de l'image rendue, en pixels (≈ 1000 dpi pour un magnet de 75 mm).
const LONG_SIDE_PX: f64 = 3000.0;

pub fn is_pdf(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::HSTRING;
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Storage::StorageFile;
    use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};

    fn err(e: windows::core::Error) -> String {
        e.message().to_string()
    }

    fn open(path: &Path) -> Result<PdfDocument, String> {
        let abs = std::path::absolute(path).map_err(|e| e.to_string())?;
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(abs.as_os_str()))
            .and_then(|op| op.join())
            .map_err(err)?;
        PdfDocument::LoadFromFileAsync(&file).and_then(|op| op.join()).map_err(|e| {
            if e.code().0 as u32 == 0x8007_052B {
                "PDF protégé par un mot de passe".to_string()
            } else {
                err(e)
            }
        })
    }

    pub fn page_count(path: &Path) -> Result<u32, String> {
        open(path)?.PageCount().map_err(err)
    }

    pub fn render(path: &Path, index: u32) -> Result<RgbaImage, String> {
        let doc = open(path)?;
        let page = doc.GetPage(index).map_err(err)?;
        let size = page.Size().map_err(err)?;
        let (w, h) = (size.Width.max(1.0) as f64, size.Height.max(1.0) as f64);
        let k = LONG_SIDE_PX / w.max(h);
        let opts = PdfPageRenderOptions::new().map_err(err)?;
        opts.SetDestinationWidth((w * k).round() as u32).map_err(err)?;
        opts.SetDestinationHeight((h * k).round() as u32).map_err(err)?;
        let stream = InMemoryRandomAccessStream::new().map_err(err)?;
        page.RenderWithOptionsToStreamAsync(&stream, &opts)
            .and_then(|op| op.join())
            .map_err(err)?;
        // Le rendu est un PNG dans le flux mémoire.
        let len = stream.Size().map_err(err)? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(err)?).map_err(err)?;
        reader.LoadAsync(len).and_then(|op| op.join()).map_err(err)?;
        let mut buf = vec![0u8; len as usize];
        reader.ReadBytes(&mut buf).map_err(err)?;
        image::load_from_memory(&buf).map(|i| i.to_rgba8()).map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::process::Command;

    const MISSING: &str = "pour lire les PDF, installez Poppler (Linux : paquet poppler-utils ; Mac : brew install poppler)";

    /// Outil de Poppler : dans le PATH, ou dans les dossiers de Homebrew (une appli lancée
    /// depuis le Finder ne voit pas le PATH du terminal).
    fn tool(name: &str) -> Command {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
            let p = Path::new(dir).join(name);
            if p.is_file() {
                return Command::new(p);
            }
        }
        Command::new(name)
    }

    fn run(mut cmd: Command) -> Result<Vec<u8>, String> {
        let out = cmd.env("LC_ALL", "C").env("LANG", "C").output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { MISSING.to_string() } else { e.to_string() }
        })?;
        if out.status.success() {
            return Ok(out.stdout);
        }
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if msg.to_lowercase().contains("password") {
            Err("PDF protégé par un mot de passe".into())
        } else if msg.is_empty() {
            Err("PDF illisible".into())
        } else {
            Err(msg)
        }
    }

    pub fn page_count(path: &Path) -> Result<u32, String> {
        let mut cmd = tool("pdfinfo");
        cmd.arg(path);
        let out = run(cmd)?;
        String::from_utf8_lossy(&out)
            .lines()
            .find_map(|l| l.strip_prefix("Pages:").and_then(|n| n.trim().parse().ok()))
            .ok_or_else(|| "nombre de pages introuvable".to_string())
    }

    pub fn render(path: &Path, index: u32) -> Result<RgbaImage, String> {
        // Fichier de sortie unique : plusieurs pages peuvent être rendues en même temps.
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join("planche-contact");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let base = dir.join(format!("pdf-{}-{n}", std::process::id()));
        let page = (index + 1).to_string();
        let mut cmd = tool("pdftoppm");
        // -scale-to : le côté le plus long de la page fait LONG_SIDE_PX pixels.
        cmd.args(["-png", "-singlefile", "-f", &page, "-l", &page, "-scale-to"])
            .arg((LONG_SIDE_PX as u32).to_string())
            .arg(path)
            .arg(&base);
        run(cmd)?;
        let png = base.with_extension("png");
        let img = image::open(&png).map(|i| i.to_rgba8()).map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&png);
        img
    }
}

/// Nombre de pages du PDF.
pub fn page_count(path: &Path) -> Result<u32, String> {
    run(path, |p| imp::page_count(p))
}

/// Page `index` (à partir de 0) rendue en image.
pub fn render(path: &Path, index: u32) -> Result<RgbaImage, String> {
    run(path, move |p| imp::render(p, index))
}

/// Exécute dans un fil à part : les appels Windows attendent des opérations asynchrones,
/// et les outils de Poppler sont des programmes externes ; on évite de bloquer la fenêtre.
fn run<T: Send>(path: &Path, f: impl FnOnce(&Path) -> Result<T, String> + Send) -> Result<T, String> {
    std::thread::scope(|s| s.spawn(|| f(path)).join().unwrap_or_else(|_| Err("erreur interne du lecteur PDF".into())))
}

#[cfg(test)]
mod tests {
    #[test]
    fn render_a_pdf() {
        let Ok(p) = std::env::var("PDF_TEST") else { return };
        let p = std::path::Path::new(&p);
        let n = super::page_count(p).unwrap();
        let img = super::render(p, 0).unwrap();
        println!("{n} page(s), page 1 = {} x {}", img.width(), img.height());
        img.save(std::env::temp_dir().join("pdf-page1.png")).unwrap();
    }
}
