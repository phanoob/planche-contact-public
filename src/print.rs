//! Impression directe via Windows (System.Drawing.Printing pilotée par PowerShell).

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

/// Ouvre la boîte d'impression Windows (papier, qualité, miroir…) puis envoie les pages\n/// (PNG A4, 300 dpi). `printer` est l'imprimante proposée au départ.
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


