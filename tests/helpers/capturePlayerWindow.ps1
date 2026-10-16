param([long]$WindowHandle, [string]$OutputPath)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PlayerCapture {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
}
'@
[void][PlayerCapture]::SetThreadDpiAwarenessContext([IntPtr](-4))
$rect = New-Object PlayerCapture+Rect
if (-not [PlayerCapture]::GetWindowRect([IntPtr]$WindowHandle, [ref]$rect)) { throw 'Probe window not found.' }
$bitmap = New-Object System.Drawing.Bitmap(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$dc = $graphics.GetHdc()
try {
  if (-not [PlayerCapture]::PrintWindow([IntPtr]$WindowHandle, $dc, 2)) { throw 'Probe window capture failed.' }
} finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
try { $bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png) } finally { $bitmap.Dispose() }
