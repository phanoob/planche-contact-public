//! Impression directe.
//! - Windows : boîte d'impression du système (System.Drawing.Printing pilotée par PowerShell),
//!   pages envoyées en PNG A4 à 300 dpi.
//! - Mac et Linux : CUPS (`lpstat`, `lp`), planche envoyée en PDF A4, sans mise à l'échelle.

use std::path::Path;
use std::process::Command;

#[cfg(windows)]
fn hidden(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
fn hidden(cmd: &mut Command) -> &mut Command {
    cmd
}

/// Renvoie (liste des imprimantes, imprimante par défaut).
#[cfg(windows)]
pub fn list_printers() -> (Vec<String>, String) {
    let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
        Add-Type -AssemblyName System.Drawing; \
        (New-Object System.Drawing.Printing.PrinterSettings).PrinterName; \
        '---'; \
        [System.Drawing.Printing.PrinterSettings]::InstalledPrinters";
    let mut cmd = Command::new("powershell");
    hidden(&mut cmd).args(["-NoProfile", "-Command", script]);
    let Ok(out) = cmd.output() else {
        return (vec![], String::new());
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
    let default = lines.next().unwrap_or_default();
    let mut list = Vec::new();
    for l in lines {
        if l != "---" {
            list.push(l);
        }
    }
    (list, default)
}

#[cfg(windows)]
const PRINT_SCRIPT: &str = r#"
param([string]$Printer, [string]$Files)
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$list = $Files.Split('|')
$script:i = 0
$doc = New-Object System.Drawing.Printing.PrintDocument
if ($Printer) { $doc.PrinterSettings.PrinterName = $Printer }
if (-not $doc.PrinterSettings.IsValid) { Write-Error 'Imprimante introuvable'; exit 2 }
foreach ($ps in $doc.PrinterSettings.PaperSizes) { if ($ps.Kind -eq 'A4') { $doc.DefaultPageSettings.PaperSize = $ps; break } }
$doc.DefaultPageSettings.Margins = New-Object System.Drawing.Printing.Margins(0,0,0,0)
$doc.add_PrintPage({
  param($sender, $e)
  $img = [System.Drawing.Image]::FromFile($list[$script:i])
  $g = $e.Graphics
  $g.PageUnit = [System.Drawing.GraphicsUnit]::Millimeter
  $hx = $e.PageSettings.HardMarginX * 25.4 / 100
  $hy = $e.PageSettings.HardMarginY * 25.4 / 100
  $g.DrawImage($img, [single](-$hx), [single](-$hy), [single]210, [single]297)
  $img.Dispose()
  $script:i++
  $e.HasMorePages = ($script:i -lt $list.Count)
})
$owner = New-Object System.Windows.Forms.Form
$owner.TopMost = $true; $owner.ShowInTaskbar = $false; $owner.Opacity = 0; $owner.StartPosition = 'CenterScreen'
$owner.Show()
$dlg = New-Object System.Windows.Forms.PrintDialog
$dlg.Document = $doc
$dlg.UseEXDialog = $false
$dlg.AllowPrintToFile = $false
$dlg.AllowSomePages = $false
$dlg.AllowSelection = $false
$res = $dlg.ShowDialog($owner)
$owner.Close()
if ($res -ne [System.Windows.Forms.DialogResult]::OK) { exit 3 }
$doc.Print()
"#;

/// Ouvre la boîte d'impression Windows (papier, qualité, miroir…) puis envoie les pages
/// (PNG A4, 300 dpi). `printer` est l'imprimante proposée au départ.
#[cfg(windows)]
pub fn print_files(printer: &str, files: &[std::path::PathBuf], dir: &Path) -> Result<(), String> {
    let script = dir.join("imprimer.ps1");
    std::fs::write(&script, PRINT_SCRIPT).map_err(|e| e.to_string())?;
    let joined = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("|");
    let mut cmd = Command::new("powershell");
    hidden(&mut cmd)
        .args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Printer")
        .arg(printer)
        .arg("-Files")
        .arg(joined);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else if out.status.code() == Some(3) {
        Err("annulée".into())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Sous Windows, `print_files` attend des pages PNG A4 à 300 dpi.
#[cfg(windows)]
pub const SEND_PDF: bool = false;
/// Mac et Linux : un seul PDF A4 (profil ICC embarqué), imprimé tel quel par CUPS.
#[cfg(not(windows))]
pub const SEND_PDF: bool = true;

/// Mac et Linux : sortie d'une commande CUPS, en anglais (textes stables à lire).
#[cfg(not(windows))]
fn cups(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).env("LC_ALL", "C").env("LANG", "C").output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Renvoie (liste des imprimantes, imprimante par défaut), d'après CUPS.
#[cfg(not(windows))]
pub fn list_printers() -> (Vec<String>, String) {
    // « NOM accepting requests since … » : le nom est le premier mot.
    let list: Vec<String> = cups("lpstat", &["-a"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    // « system default destination: NOM »
    let default = cups("lpstat", &["-d"])
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.split_once("destination:").map(|(_, n)| n.trim().to_string()))
        .unwrap_or_default();
    (list, default)
}

/// Envoie les fichiers (PDF A4) à l'imprimante par `lp`, à taille réelle.
/// Pas de boîte d'impression : papier et qualité sont les réglages par défaut de l'imprimante
/// (préférences du système, ou interface de CUPS sur http://localhost:631).
#[cfg(not(windows))]
pub fn print_files(printer: &str, files: &[std::path::PathBuf], _dir: &Path) -> Result<(), String> {
    let mut cmd = Command::new("lp");
    hidden(&mut cmd).env("LC_ALL", "C");
    if !printer.is_empty() {
        cmd.arg("-d").arg(printer);
    }
    cmd.args(["-o", "media=A4", "-o", "print-scaling=none", "-o", "fit-to-page=false"]);
    cmd.args(files);
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "commande « lp » introuvable : installez CUPS (paquet cups-client sous Linux)".to_string()
        } else {
            e.to_string()
        }
    })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
