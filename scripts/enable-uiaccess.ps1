# Run elevated with Windows PowerShell 5.1 on the target machine, with Robocam closed.
[CmdletBinding()]
param(
    [switch]$Provision,
    [switch]$Reapply,
    [string]$Executable = (Join-Path $env:ProgramFiles 'robocam\robocam.exe')
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($Provision.IsPresent -eq $Reapply.IsPresent) { throw 'Choose -Provision or -Reapply.' }
$configuration = 'HKLM:\SOFTWARE\robocam\UIAccess'
$thumbprint = Get-ItemPropertyValue -LiteralPath $configuration -Name CertificateThumbprint -ErrorAction SilentlyContinue
if ($Reapply -and !$thumbprint) {
    Write-Host 'UIAccess is not provisioned on this machine.'
    return
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (!(New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'UIAccess provisioning and signing require an elevated administrator.'
}
$Executable = [IO.Path]::GetFullPath($Executable)
$secureRoot = [IO.Path]::GetFullPath($env:ProgramFiles).TrimEnd('\') + '\'
if (!$Executable.StartsWith($secureRoot, [StringComparison]::OrdinalIgnoreCase) -or
    [IO.Path]::GetFileName($Executable) -ine 'robocam.exe') {
    throw 'UIAccess requires robocam.exe installed under Program Files.'
}
$entry = Get-Item -LiteralPath $Executable
while ($entry) {
    if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'UIAccess paths must not contain junctions or symbolic links.' }
    $entry = if ($entry -is [IO.FileInfo]) { $entry.Directory } else { $entry.Parent }
}
$info = (Get-Item -LiteralPath $Executable).VersionInfo
if ($info.ProductName -cne 'robocam' -or $info.OriginalFilename -cne 'robocam.exe') { throw 'Not a Robocam executable.' }
if (Get-Process robocam -ErrorAction SilentlyContinue | Where-Object { $_.Path -ieq $Executable }) {
    throw 'Close Robocam cleanly with --request-exit in its interactive session before signing.'
}
if (!$thumbprint) {
    $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=Robocam UIAccess on $env:COMPUTERNAME" `
        -FriendlyName 'Robocam UIAccess (machine-local)' -CertStoreLocation Cert:\LocalMachine\My `
        -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy NonExportable `
        -NotAfter (Get-Date).AddYears(3)
    $thumbprint = $certificate.Thumbprint
} else {
    $certificate = Get-Item -LiteralPath "Cert:\LocalMachine\My\$thumbprint"
}
if (!$certificate.HasPrivateKey -or $certificate.NotAfter -le (Get-Date) -or $certificate.NotBefore -gt (Get-Date) -or
    @($certificate.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' }).Count -eq 0) {
    throw 'The configured certificate must have a valid code-signing private key.'
}
if ($Provision) {
    # Trust only the public certificate. The non-exportable private key stays in
    # the local machine store, protected by Windows, outside application packages.
    foreach ($storeName in @('Root', 'TrustedPublisher')) {
        $store = New-Object Security.Cryptography.X509Certificates.X509Store($storeName, 'LocalMachine')
        try { $store.Open('ReadWrite'); $store.Add($certificate) } finally { $store.Close() }
    }
}
Add-Type -ReferencedAssemblies 'System.dll', 'System.Core.dll', 'System.Xml.dll' -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Xml;
public static class RobocamManifest {
    delegate bool LanguageCallback(IntPtr module, IntPtr type, IntPtr name, ushort language, IntPtr parameter);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool EnumResourceLanguages(IntPtr module, IntPtr type, IntPtr name, LanguageCallback callback, IntPtr parameter);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr FindResourceEx(IntPtr module, IntPtr type, IntPtr name, ushort language);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr BeginUpdateResource(string path, bool delete);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool UpdateResource(IntPtr update, IntPtr type, IntPtr name, ushort language, byte[] data, uint length);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool EndUpdateResource(IntPtr update, bool discard);
    static void Check(bool success) { if (!success) throw new Win32Exception(); }
    public static void Enable(string path) {
        var manifests = new Dictionary<ushort, byte[]>();
        IntPtr module = LoadLibraryEx(path, IntPtr.Zero, 0x22); // data and image resource, never execute code
        Check(module != IntPtr.Zero);
        try {
            LanguageCallback callback = (m, t, n, language, p) => {
                IntPtr resource = FindResourceEx(m, t, n, language);
                Check(resource != IntPtr.Zero);
                uint size = SizeofResource(m, resource);
                Check(size > 0);
                IntPtr data = LockResource(LoadResource(m, resource));
                Check(data != IntPtr.Zero);
                var bytes = new byte[size];
                Marshal.Copy(data, bytes, 0, bytes.Length);
                manifests.Add(language, bytes);
                return true;
            };
            Check(EnumResourceLanguages(module, (IntPtr)24, (IntPtr)1, callback, IntPtr.Zero));
        } finally { FreeLibrary(module); }
        var replacements = new Dictionary<ushort, byte[]>();
        foreach (var manifest in manifests) {
            var document = new XmlDocument { PreserveWhitespace = true, XmlResolver = null };
            using (var input = new MemoryStream(manifest.Value)) { document.Load(input); }
            var namespaces = new XmlNamespaceManager(document.NameTable);
            namespaces.AddNamespace("v3", "urn:schemas-microsoft-com:asm.v3");
            var nodes = document.SelectNodes("//v3:requestedExecutionLevel", namespaces);
            if (nodes.Count != 1) throw new InvalidDataException("Expected one execution-level declaration.");
            ((XmlElement)nodes[0]).SetAttribute("uiAccess", "true");
            using (var output = new MemoryStream()) {
                using (var writer = XmlWriter.Create(output, new XmlWriterSettings { Encoding = new UTF8Encoding(false), CloseOutput = false })) { document.Save(writer); }
                replacements.Add(manifest.Key, output.ToArray());
            }
        }
        IntPtr update = BeginUpdateResource(path, false);
        Check(update != IntPtr.Zero);
        bool committed = false;
        try {
            foreach (var replacement in replacements) {
                Check(UpdateResource(update, (IntPtr)24, (IntPtr)1, replacement.Key, replacement.Value, (uint)replacement.Value.Length));
            }
            bool result = EndUpdateResource(update, false);
            update = IntPtr.Zero;
            Check(result);
            committed = true;
        } finally { if (!committed && update != IntPtr.Zero) EndUpdateResource(update, true); }
    }
}
'@
$temporary = Join-Path (Split-Path -Parent $Executable) ('uiaccess-' + [Guid]::NewGuid().ToString('N') + '.exe')
try {
    Copy-Item -LiteralPath $Executable -Destination $temporary
    [RobocamManifest]::Enable($temporary)
    $signature = Set-AuthenticodeSignature -LiteralPath $temporary -Certificate $certificate -HashAlgorithm SHA256
    $verified = Get-AuthenticodeSignature -LiteralPath $temporary
    if ($signature.Status -ne 'Valid' -or $verified.Status -ne 'Valid' -or $verified.SignerCertificate.Thumbprint -ine $thumbprint) {
        throw "UIAccess signature verification failed: $($verified.StatusMessage)"
    }
    Move-Item -LiteralPath $temporary -Destination $Executable -Force
    New-Item -Path $configuration -Force | Out-Null
    New-ItemProperty -LiteralPath $configuration -Name CertificateThumbprint -Value $thumbprint -PropertyType String -Force | Out-Null
    Write-Host "UIAccess enabled: $Executable; certificate $thumbprint; expires $($certificate.NotAfter.ToString('yyyy-MM-dd'))."
} finally {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
}
