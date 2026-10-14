# Reproduce the original geometric camera icon using Windows GDI+ (no external tools).
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$repository = Split-Path -Parent $PSScriptRoot
$bitmap = New-Object Drawing.Bitmap 256,256
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
$dark = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#122a35'))
$teal = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#39d9be'))
$white = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#edfefa'))
function RoundedRectangle([int]$X,[int]$Y,[int]$Width,[int]$Height,[int]$Radius) {
    $path = New-Object Drawing.Drawing2D.GraphicsPath
    $diameter = 2*$Radius
    $path.AddArc($X,$Y,$diameter,$diameter,180,90)
    $path.AddArc($X+$Width-$diameter,$Y,$diameter,$diameter,270,90)
    $path.AddArc($X+$Width-$diameter,$Y+$Height-$diameter,$diameter,$diameter,0,90)
    $path.AddArc($X,$Y+$Height-$diameter,$diameter,$diameter,90,90)
    $path.CloseFigure()
    return $path
}
$background = RoundedRectangle 8 8 240 240 56
$body = RoundedRectangle 44 84 168 120 20
$graphics.FillPath($dark,$background)
$graphics.FillPath($teal,$body)
$points = [Drawing.Point[]]@([Drawing.Point]::new(84,84),[Drawing.Point]::new(96,64),[Drawing.Point]::new(160,64),[Drawing.Point]::new(172,84))
$graphics.FillPolygon($teal,$points)
$graphics.FillEllipse($dark,85,97,86,86)
$graphics.FillEllipse($white,100,112,56,56)
$graphics.FillEllipse($teal,124,120,24,24)
$graphics.FillEllipse($dark,180,102,14,14)
$sizes = @(16,24,32,48,64,128,256)
$images = @()
foreach ($size in $sizes) {
    $resized = New-Object Drawing.Bitmap $size,$size
    $resampler = [Drawing.Graphics]::FromImage($resized)
    $resampler.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $resampler.DrawImage($bitmap,0,0,$size,$size)
    $stream = New-Object IO.MemoryStream
    $resized.Save($stream,[Drawing.Imaging.ImageFormat]::Png)
    $images += ,$stream.ToArray()
    $stream.Dispose(); $resampler.Dispose(); $resized.Dispose()
}
$output = [IO.File]::Create((Join-Path $repository 'assets\robocam.ico'))
$writer = New-Object IO.BinaryWriter $output
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
$offset = 6 + 16*$sizes.Count
for ($index = 0; $index -lt $sizes.Count; $index++) {
    $sizeByte = [byte]($sizes[$index] % 256)
    $writer.Write($sizeByte); $writer.Write($sizeByte)
    $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([uint16]1); $writer.Write([uint16]32)
    $writer.Write([uint32]$images[$index].Length); $writer.Write([uint32]$offset)
    $offset += $images[$index].Length
}
foreach ($png in $images) { $writer.Write([byte[]]$png) }
$writer.Dispose(); $output.Dispose()
$background.Dispose(); $body.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
$dark.Dispose(); $teal.Dispose(); $white.Dispose()
