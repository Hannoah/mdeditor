# Windows app.ico generator for mdeditor (Multi-frame 256, 128, 64, 48, 32, 24, 16)
Add-Type -AssemblyName System.Drawing

function Create-AppFrame([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    # Padding and geometry
    [float]$pad = [Math]::Max([float]1.0, [float]($size * 0.06))
    [float]$w = [float]($size - (2.0 * $pad))
    $rect = New-Object System.Drawing.RectangleF($pad, $pad, $w, $w)
    [float]$radius = [Math]::Max([float]2.0, [float]($size * 0.22))
    [float]$d = [float]($radius * 2.0)

    # Squircle rounded rect path
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($rect.X, $rect.Y, $d, $d, [float]180, [float]90)
    $path.AddArc([float]($rect.Right - $d), $rect.Y, $d, $d, [float]270, [float]90)
    $path.AddArc([float]($rect.Right - $d), [float]($rect.Bottom - $d), $d, $d, [float]0, [float]90)
    $path.AddArc($rect.X, [float]($rect.Bottom - $d), $d, $d, [float]90, [float]90)
    $path.CloseFigure()

    # Dark Obsidian / Midnight Gradient (#0f172a -> #030712)
    $cTop = [System.Drawing.Color]::FromArgb(255, 24, 33, 47)
    $cBottom = [System.Drawing.Color]::FromArgb(255, 7, 10, 18)
    $bgBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $cTop, $cBottom, [single]55.0)
    $g.FillPath($bgBrush, $path)
    $bgBrush.Dispose()

    # Subtle 1px ambient border (#38bdf8 accent with soft transparency)
    [float]$borderWidth = [Math]::Max([float]1.0, [float]($size * 0.025))
    $borderPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(180, 56, 189, 248), $borderWidth)
    $g.DrawPath($borderPen, $path)
    $borderPen.Dispose()
    $path.Dispose()

    # Glyphs: Markdown Monogram "M" (left) and Downward Arrow "↓" (right)
    [float]$glyphTop = [float]($size * 0.28)
    [float]$glyphBottom = [float]($size * 0.72)
    [float]$glyphH = [float]($glyphBottom - $glyphTop)
    [float]$stroke = [Math]::Max([float]1.5, [float]($size * 0.082))

    # M pen (pure crisp white)
    $mPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 248, 250, 252), $stroke)
    $mPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $mPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $mPen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round

    [float]$mLeft = [float]($size * 0.20)
    [float]$mRight = [float]($size * 0.58)
    [float]$mMid = [float](($mLeft + $mRight) / 2.0)
    [float]$mDrop = [float]($glyphTop + ($glyphH * 0.65))

    [System.Drawing.PointF[]]$mPoints = @(
        (New-Object System.Drawing.PointF($mLeft, $glyphBottom)),
        (New-Object System.Drawing.PointF($mLeft, $glyphTop)),
        (New-Object System.Drawing.PointF($mMid, $mDrop)),
        (New-Object System.Drawing.PointF($mRight, $glyphTop)),
        (New-Object System.Drawing.PointF($mRight, $glyphBottom))
    )
    $g.DrawLines($mPen, $mPoints)
    $mPen.Dispose()

    # Down Arrow pen (Cyan #38bdf8)
    $arrPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 56, 189, 248), $stroke)
    $arrPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $arrPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $arrPen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round

    [float]$arrX = [float]($size * 0.74)
    $g.DrawLine($arrPen, $arrX, $glyphTop, $arrX, $glyphBottom)

    # Arrow head (V)
    [float]$arrHeadW = [float]($size * 0.085)
    [float]$arrHeadH = [float]($glyphH * 0.32)
    [System.Drawing.PointF[]]$arrowHead = @(
        (New-Object System.Drawing.PointF([float]($arrX - $arrHeadW), [float]($glyphBottom - $arrHeadH))),
        (New-Object System.Drawing.PointF($arrX, $glyphBottom)),
        (New-Object System.Drawing.PointF([float]($arrX + $arrHeadW), [float]($glyphBottom - $arrHeadH)))
    )
    $g.DrawLines($arrPen, $arrowHead)
    $arrPen.Dispose()

    $g.Dispose()
    return $bmp
}

# Icon sizes to generate
$sizes = @(256, 128, 64, 48, 32, 24, 16)
$pngDataList = @()

foreach ($s in $sizes) {
    $bmp = Create-AppFrame -size $s
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $pngDataList += ,($ms.ToArray())
    $ms.Dispose()
}

# Write multi-frame ICO file
$icoPath = Join-Path $PSScriptRoot "app.ico"
$fs = [System.IO.File]::Create($icoPath)
$binWriter = New-Object System.IO.BinaryWriter($fs)

# ICONDIR header: idReserved (0), idType (1 for ICO), idCount
$binWriter.Write([uint16]0)
$binWriter.Write([uint16]1)
$binWriter.Write([uint16]$sizes.Length)

# Calculate image data offset
# Header is 6 bytes, each ICONDIRENTRY is 16 bytes
[uint32]$offset = 6 + (16 * $sizes.Length)

for ($i = 0; $i -lt $sizes.Length; $i++) {
    $s = $sizes[$i]
    [uint32]$dataLen = $pngDataList[$i].Length

    [byte]$bWidth = 0
    if ($s -lt 256) { $bWidth = [byte]$s }

    $binWriter.Write($bWidth)
    $binWriter.Write($bWidth)
    $binWriter.Write([byte]0)   # bColorCount
    $binWriter.Write([byte]0)   # bReserved
    $binWriter.Write([uint16]1) # wPlanes
    $binWriter.Write([uint16]32)# wBitCount (32-bit ARGB PNG)
    $binWriter.Write($dataLen)  # dwBytesInRes
    $binWriter.Write($offset)   # dwImageOffset

    $offset += $dataLen
}

# Write PNG bytes
for ($i = 0; $i -lt $sizes.Length; $i++) {
    $rawBytes = $pngDataList[$i]
    $binWriter.Write($rawBytes, 0, $rawBytes.Length)
}

$binWriter.Flush()
$binWriter.Close()
$fs.Close()

Write-Host "Generated modern multi-resolution app.ico successfully ($((Get-Item $icoPath).Length) bytes)."
