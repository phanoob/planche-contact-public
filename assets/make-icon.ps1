# Génère l'icône de Planche Contact : assets\icon.png (256 px) et assets\icon.ico (16 à 256 px).
# Utilisation : powershell -File assets\make-icon.ps1
Add-Type -AssemblyName System.Drawing
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path

function RoundRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function Draw([int]$S) {
    $bmp = New-Object System.Drawing.Bitmap $S, $S, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.PixelOffsetMode = 'HighQuality'
    $k = $S / 256.0

    # Fond orange arrondi, léger dégradé.
    $bg = RoundRect (8 * $k) (8 * $k) (240 * $k) (240 * $k) (52 * $k)
    $rect = New-Object System.Drawing.RectangleF 0, 0, $S, $S
    $grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, ([System.Drawing.Color]::FromArgb(246, 146, 82)), ([System.Drawing.Color]::FromArgb(214, 88, 38)), 60
    $g.FillPath($grad, $bg)

    # Feuille A4 avec ombre.
    $sx = 60 * $k; $sy = 34 * $k; $sw = 136 * $k; $sh = 188 * $k
    $sh1 = RoundRect ($sx + 4 * $k) ($sy + 7 * $k) $sw $sh (10 * $k)
    $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(70, 90, 30, 0))), $sh1)
    $sheet = RoundRect $sx $sy $sw $sh (10 * $k)
    $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 252, 248))), $sheet)

    # Magnets : 2 colonnes x 3 rangées (2 x 2 en tout petit).
    $cols = @(
        [System.Drawing.Color]::FromArgb(58, 120, 140), [System.Drawing.Color]::FromArgb(236, 120, 62),
        [System.Drawing.Color]::FromArgb(122, 154, 92), [System.Drawing.Color]::FromArgb(110, 150, 205),
        [System.Drawing.Color]::FromArgb(170, 110, 160), [System.Drawing.Color]::FromArgb(90, 96, 110))
    $rows = if ($S -le 24) { 2 } else { 3 }
    $m = 14 * $k; $gap = 9 * $k
    $tw = ($sw - 2 * $m - $gap) / 2
    $th = ($sh - 2 * $m - ($rows - 1) * $gap) / $rows
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(40, 30, 25)), ([Math]::Max(1.0, 2.2 * $k))
    if ($S -ge 48) { $pen.DashPattern = @(2.2, 1.6) }
    $i = 0
    for ($r = 0; $r -lt $rows; $r++) {
        for ($c = 0; $c -lt 2; $c++) {
            $x = $sx + $m + $c * ($tw + $gap); $y = $sy + $m + $r * ($th + $gap)
            $t = RoundRect $x $y $tw $th (5 * $k)
            $g.FillPath((New-Object System.Drawing.SolidBrush $cols[$i]), $t)
            if ($S -ge 32) { $g.DrawPath($pen, $t) }
            $i++
        }
    }
    $g.Dispose()
    return $bmp
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = @()
foreach ($s in $sizes) {
    $b = Draw $s
    $ms = New-Object System.IO.MemoryStream
    if ($s -eq 256) {
        # 256 px en PNG (format accepté par Windows à cette taille).
        $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $b.Save((Join-Path $dir 'icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    } else {
        # Petites tailles en BMP 32 bits (lu par tous les lecteurs d'icônes).
        $bw = New-Object System.IO.BinaryWriter $ms
        $bw.Write([uint32]40); $bw.Write([int32]$s); $bw.Write([int32]($s * 2))
        $bw.Write([uint16]1); $bw.Write([uint16]32); $bw.Write([uint32]0)
        $bw.Write([uint32]0); $bw.Write([int32]0); $bw.Write([int32]0); $bw.Write([uint32]0); $bw.Write([uint32]0)
        $data = $b.LockBits((New-Object System.Drawing.Rectangle 0, 0, $s, $s), 'ReadOnly', ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb))
        $px = New-Object byte[] ($s * $s * 4)
        [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length)
        $b.UnlockBits($data)
        for ($y = $s - 1; $y -ge 0; $y--) { $bw.Write($px, $y * $s * 4, $s * 4) }
        $maskRow = [int]([Math]::Ceiling($s / 32.0) * 4)
        $bw.Write((New-Object byte[] ($maskRow * $s)))
        $bw.Flush()
    }
    $b.Dispose()
    $pngs += , $ms.ToArray()
}

# Fichier ICO : en-tête + répertoire + images PNG.
$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]; $len = $pngs[$i].Length
    $w.Write([byte]($s % 256)); $w.Write([byte]($s % 256)); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$len); $w.Write([uint32]$offset)
    $offset += $len
}
foreach ($p in $pngs) { $w.Write($p) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $dir 'icon.ico'), $out.ToArray())
"icon.png et icon.ico générés"
