param([long]$WindowHandle, [ValidateSet('doubleClick','space','left','right','enter','escape','backspace')][string]$Action, [int]$DoubleClickInterval=350)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class PlayerInput {
  public delegate bool Visitor(IntPtr hwnd, IntPtr param);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hwnd, Visitor callback, IntPtr param);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder name, int max);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
}
'@
$script:video = [IntPtr]::Zero
$visitor = [PlayerInput+Visitor]{param($hwnd,$unused)
  $name=New-Object System.Text.StringBuilder 256
  [void][PlayerInput]::GetClassName($hwnd,$name,256)
  if ($name.ToString() -like '*mpv*') { $script:video=$hwnd; return $false }
  return $true
}
[void][PlayerInput]::EnumChildWindows([IntPtr]$WindowHandle,$visitor,[IntPtr]::Zero)
if ($script:video -eq [IntPtr]::Zero) { throw 'The isolated probe has no native mpv window.' }
if ($Action -eq 'doubleClick') {
  # mpv detects double clicks from two press/release pairs itself.
  [void][PlayerInput]::PostMessage($script:video,0x200,[IntPtr]0,[IntPtr]0x00640064)
  [void][PlayerInput]::PostMessage($script:video,0x201,[IntPtr]1,[IntPtr]0x00640064)
  [void][PlayerInput]::PostMessage($script:video,0x202,[IntPtr]0,[IntPtr]0x00640064)
  Start-Sleep -Milliseconds $DoubleClickInterval
  [void][PlayerInput]::PostMessage($script:video,0x201,[IntPtr]1,[IntPtr]0x00640064)
  [void][PlayerInput]::PostMessage($script:video,0x202,[IntPtr]0,[IntPtr]0x00640064)
} else {
  $key=@{space=0x20;left=0x25;right=0x27;enter=0x0D;escape=0x1B;backspace=0x08}[$Action]
  [void][PlayerInput]::PostMessage($script:video,0x100,[IntPtr]$key,[IntPtr]1)
  [void][PlayerInput]::PostMessage($script:video,0x101,[IntPtr]$key,[IntPtr]0xC0000001L)
}
