$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class Wave5bHandlePoc {
 [StructLayout(LayoutKind.Sequential)] public struct Info { public uint Attr; public System.Runtime.InteropServices.ComTypes.FILETIME C,A,W; public uint Volume,SizeHi,SizeLo,Links,IndexHi,IndexLow; }
 [StructLayout(LayoutKind.Sequential)] public struct Basic { public long C,A,W,Ch; public uint Attr,Reserved; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern SafeFileHandle CreateFileW(string n,uint a,uint s,IntPtr sec,uint disp,uint flags,IntPtr t);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool GetFileInformationByHandle(SafeFileHandle h,out Info i);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool SetFileInformationByHandle(SafeFileHandle h,int c,ref bool i,uint z);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool SetFileInformationByHandle(SafeFileHandle h,int c,ref Basic i,uint z);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool DeviceIoControl(SafeFileHandle h,uint code,IntPtr input,uint inputSize,IntPtr output,uint outputSize,out uint returned,IntPtr overlapped);
 public static SafeFileHandle Open(string p,bool d,uint a,uint s=1) => CreateFileW(p,a,s,IntPtr.Zero,3,0x00200000u|(d?0x02000000u:0),IntPtr.Zero);
 public static int SetJunctionError(SafeFileHandle h,string target) {
  string sub="\\??\\"+target+"\\", print=target; byte[] a=System.Text.Encoding.Unicode.GetBytes(sub), b=System.Text.Encoding.Unicode.GetBytes(print);
  byte[] path=new byte[a.Length+b.Length]; Buffer.BlockCopy(a,0,path,0,a.Length); Buffer.BlockCopy(b,0,path,a.Length,b.Length);
  byte[] buf=new byte[16+path.Length]; Buffer.BlockCopy(BitConverter.GetBytes(0xA0000003u),0,buf,0,4); Buffer.BlockCopy(BitConverter.GetBytes((ushort)(8+path.Length)),0,buf,4,2);
  Buffer.BlockCopy(BitConverter.GetBytes((ushort)0),0,buf,6,2); Buffer.BlockCopy(BitConverter.GetBytes((ushort)0),0,buf,8,2); Buffer.BlockCopy(BitConverter.GetBytes((ushort)a.Length),0,buf,10,2);
  Buffer.BlockCopy(BitConverter.GetBytes((ushort)a.Length),0,buf,12,2); Buffer.BlockCopy(BitConverter.GetBytes((ushort)b.Length),0,buf,14,2); Buffer.BlockCopy(path,0,buf,16,path.Length);
  IntPtr p=Marshal.AllocHGlobal(buf.Length); try { Marshal.Copy(buf,0,p,buf.Length); uint n; bool ok=DeviceIoControl(h,0x000900A4u,p,(uint)buf.Length,IntPtr.Zero,0,out n,IntPtr.Zero); return ok?0:Marshal.GetLastWin32Error(); } finally { Marshal.FreeHGlobal(p); }
 }
}
'@
function Invoke-VolumeProbe([string]$Volume) {
 $result=[ordered]@{volume=$Volume}
 $base=if($Volume -eq 'F:\'){$PSScriptRoot}else{[IO.Path]::GetTempPath()}
 $root=Join-Path $base ('handle-poc-work-'+$Volume.Substring(0,1)+'-'+[guid]::NewGuid().ToString('N'))
 $result.root=$root
 $result.actual_filesystem=([IO.DriveInfo]::new([IO.Path]::GetPathRoot([IO.Path]::GetFullPath($root)))).DriveFormat
 try {
  New-Item -ItemType Directory -Path $root | Out-Null
  $result.filesystem=([IO.DriveInfo]::new([IO.Path]::GetPathRoot([IO.Path]::GetFullPath($root)))).DriveFormat
  $dir=Join-Path $root 'empty'; New-Item -ItemType Directory -Path $dir | Out-Null
  $target=Join-Path $root 'target'; New-Item -ItemType Directory -Path $target | Out-Null; [IO.File]::WriteAllText((Join-Path $target 'sentinel.txt'),'outside')
  $writer=[Wave5bHandlePoc]::Open($dir,$true,0x40000000u,7); $result.preexisting_writer_opened=(-not $writer.IsInvalid)
  $guard=[Wave5bHandlePoc]::Open($dir,$true,0x10080u,1); $result.guard_rejected_existing_writer=$guard.IsInvalid
  if (-not $writer.IsInvalid) {$writer.Dispose()}; if (-not $guard.IsInvalid) {$guard.Dispose()}
  $guard=[Wave5bHandlePoc]::Open($dir,$true,0x10080u,1); if ($guard.IsInvalid) {throw "guard open failed win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"}
  $info=[Wave5bHandlePoc+Info]::new(); $result.classic_handle_info_ok=[Wave5bHandlePoc]::GetFileInformationByHandle($guard,[ref]$info)
  $result.volume_serial=('{0:X8}' -f $info.Volume); $result.file_index=('{0:X8}{1:X8}' -f $info.IndexHi,$info.IndexLow)
  try {Move-Item -LiteralPath $dir -Destination (Join-Path $root 'parked') -ErrorAction Stop; $result.rename_blocked=$false} catch {$result.rename_blocked=$true}
  $result.fsctl_guard_error=[Wave5bHandlePoc]::SetJunctionError($guard,$target)
  $low=[Wave5bHandlePoc]::Open($dir,$true,0x80u,7); $result.fsctl_readattr_error=if($low.IsInvalid){[Runtime.InteropServices.Marshal]::GetLastWin32Error()}else{[Wave5bHandlePoc]::SetJunctionError($low,$target)}; if(-not $low.IsInvalid){$low.Dispose()}
  $zero=[Wave5bHandlePoc]::Open($dir,$true,0u,7); $result.fsctl_zero_error=if($zero.IsInvalid){[Runtime.InteropServices.Marshal]::GetLastWin32Error()}else{[Wave5bHandlePoc]::SetJunctionError($zero,$target)}; if(-not $zero.IsInvalid){$zero.Dispose()}
  $writer2=[Wave5bHandlePoc]::Open($dir,$true,0x40000000u,7); $result.writer_open_blocked=$writer2.IsInvalid; if(-not $writer2.IsInvalid){$writer2.Dispose()}
  $guard.Dispose()
  $writer3=[Wave5bHandlePoc]::Open($dir,$true,0x40000000u,7); $result.unlocked_writer_opened=(-not $writer3.IsInvalid)
  $result.unlocked_fsctl_error=if($writer3.IsInvalid){[Runtime.InteropServices.Marshal]::GetLastWin32Error()}else{[Wave5bHandlePoc]::SetJunctionError($writer3,$target)}
  if(-not $writer3.IsInvalid){$writer3.Dispose()}
  $link=[Wave5bHandlePoc]::Open($dir,$true,0x10080u,1)
  if(-not $link.IsInvalid){$delete=$true; $result.link_unlink_same_handle=[Wave5bHandlePoc]::SetFileInformationByHandle($link,4,[ref]$delete,1); $result.link_unlink_error=if($result.link_unlink_same_handle){0}else{[Runtime.InteropServices.Marshal]::GetLastWin32Error()}; $link.Dispose()}
  $file=Join-Path $root 'readonly.txt'; [IO.File]::WriteAllText($file,'same-handle-delete'); [IO.File]::SetAttributes($file,[IO.FileAttributes]::ReadOnly)
  $fh=[Wave5bHandlePoc]::Open($file,$false,0x10180u,1); if($fh.IsInvalid){throw "file open failed win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"}
  $basic=[Wave5bHandlePoc+Basic]::new(); $basic.Attr=0x80; $result.readonly_basic_info_same_handle=[Wave5bHandlePoc]::SetFileInformationByHandle($fh,0,[ref]$basic,[Runtime.InteropServices.Marshal]::SizeOf($basic)); $result.basic_info_error=if($result.readonly_basic_info_same_handle){0}else{[Runtime.InteropServices.Marshal]::GetLastWin32Error()}
  $delete=$true; $result.file_disposition_same_handle=[Wave5bHandlePoc]::SetFileInformationByHandle($fh,4,[ref]$delete,1); $result.disposition_error=if($result.file_disposition_same_handle){0}else{[Runtime.InteropServices.Marshal]::GetLastWin32Error()}; $fh.Dispose()
  $result.readonly_file_exists_after_close=Test-Path $file
 } catch {$result.error=$_.Exception.Message}
 finally {if(Test-Path $root){Get-ChildItem -LiteralPath $root -Force -Recurse|ForEach-Object{if(-not $_.PSIsContainer){try{[IO.File]::SetAttributes($_.FullName,[IO.FileAttributes]::Normal)}catch{}}};Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue}}
 return [pscustomobject]$result
}
@((Invoke-VolumeProbe 'F:\'),(Invoke-VolumeProbe 'C:\')) | ConvertTo-Json -Depth 4
