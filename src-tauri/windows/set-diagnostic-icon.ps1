$ErrorActionPreference = 'Stop'
# Tauri compiles the application icon into every Cargo binary. Replace only the
# generated diagnostic executable before the bundler copies it into the installer.
if ($env:TAURI_ENV_PLATFORM -and $env:TAURI_ENV_PLATFORM -ne 'windows') { exit 0 }
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $taskRoot 'target' }
if (-not [IO.Path]::IsPathRooted($taskTarget)) { $taskTarget = Join-Path $taskRoot $taskTarget }
$taskProfile = if ($env:TAURI_ENV_DEBUG -eq 'true') { 'debug' } else { 'release' }
$taskCandidates = @((Join-Path $taskTarget "$taskProfile/scan_diagnostic.exe"))
if ($env:TAURI_ENV_TARGET_TRIPLE) { $taskCandidates += Join-Path $taskTarget "$env:TAURI_ENV_TARGET_TRIPLE/$taskProfile/scan_diagnostic.exe" }
$taskBinaries = @($taskCandidates | Where-Object { Test-Path -LiteralPath $_ })
if ($taskBinaries.Count -eq 0) { throw 'No generated scan_diagnostic.exe found in the Cargo target directory.' }
$taskIcon = Join-Path $taskRoot 'icons/scan-diagnostic.ico'

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
public static class DiagnosticIconResource {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr BeginUpdateResource(string path, bool deleteExisting);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool UpdateResource(IntPtr file, IntPtr type, IntPtr name, ushort language, byte[] data, uint size);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool EndUpdateResource(IntPtr file, bool discard);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr FindResourceEx(IntPtr module, IntPtr type, IntPtr name, ushort language);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
    public static void Replace(string executable, string icon) {
        byte[] ico = File.ReadAllBytes(icon);
        if (ico.Length < 6 || BitConverter.ToUInt16(ico,0) != 0 || BitConverter.ToUInt16(ico,2) != 1) throw new InvalidDataException("Invalid ICO header");
        int count = BitConverter.ToUInt16(ico,4);
        if (count < 1 || ico.Length < 6 + count * 16) throw new InvalidDataException("Invalid ICO directory");
        byte[] group = new byte[6 + count * 14];
        Buffer.BlockCopy(ico,0,group,0,6);
        byte[][] images = new byte[count][];
        for (int i=0;i<count;i++) {
            int entry=6+i*16;
            uint size=BitConverter.ToUInt32(ico,entry+8), offset=BitConverter.ToUInt32(ico,entry+12);
            if (size == 0 || offset > ico.Length || size > ico.Length - offset) throw new InvalidDataException("Invalid ICO image range");
            images[i] = new byte[size];
            Buffer.BlockCopy(ico,(int)offset,images[i],0,(int)size);
            Buffer.BlockCopy(ico,entry,group,6+i*14,12);
            Buffer.BlockCopy(BitConverter.GetBytes((ushort)(i+1)),0,group,6+i*14+12,2);
        }
        IntPtr module=LoadLibraryEx(executable,IntPtr.Zero,0x22);
        if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        bool found;
        try { found=FindResourceEx(module,new IntPtr(14),new IntPtr(32512),0) != IntPtr.Zero; }
        finally { FreeLibrary(module); }
        if (!found) throw new InvalidDataException("Expected Tauri neutral-language application icon resource 32512");
        IntPtr handle=BeginUpdateResource(executable,false);
        if (handle == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            for (int i=0;i<count;i++) if (!UpdateResource(handle,new IntPtr(3),new IntPtr(i+1),0,images[i],(uint)images[i].Length)) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (!UpdateResource(handle,new IntPtr(14),new IntPtr(32512),0,group,(uint)group.Length)) throw new Win32Exception(Marshal.GetLastWin32Error());
            IntPtr commit=handle;handle=IntPtr.Zero;
            if (!EndUpdateResource(commit,false)) throw new Win32Exception(Marshal.GetLastWin32Error());
        } finally { if (handle != IntPtr.Zero) EndUpdateResource(handle,true); }
    }
}
'@
# Native and explicit --target outputs can coexist. Both are disposable Cargo
# outputs; patch each so the binary selected by Tauri has the dedicated icon.
foreach ($taskBinary in $taskBinaries) { [DiagnosticIconResource]::Replace($taskBinary, $taskIcon) }
Write-Output 'Embedded the dedicated scan diagnostic icon before bundling.'
