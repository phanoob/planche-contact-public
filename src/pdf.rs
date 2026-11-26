//! Lecture des PDF : chaque page devient une image, rendue par le moteur PDF intégré à Windows
//! (Windows.Data.Pdf, aucune DLL à installer).

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
    pub fn page_count(_: &Path) -> Result<u32, String> {
        Err("lecture des PDF disponible seulement sous Windows".into())
    }
    pub fn render(_: &Path, _: u32) -> Result<RgbaImage, String> {
        Err("lecture des PDF disponible seulement sous Windows".into())
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

/// Exécute dans un fil à part : les appels Windows attendent des opérations
/// asynchrones, qu'on évite de bloquer sur le fil de la fenêtre.
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
