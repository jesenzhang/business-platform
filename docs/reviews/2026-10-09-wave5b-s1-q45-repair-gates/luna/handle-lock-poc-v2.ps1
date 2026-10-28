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
 public static byte[] GetReparseData(SafeFileHandle h) {
  byte[] data=new byte[16384]; GCHandle pin=GCHandle.Alloc(data,GCHandleType.Pinned);
  try {uint n; if(!DeviceIoControl(h,0x000900A8u,IntPtr.Zero,0,pin.AddrOfPinnedObject(),(uint)data.Length,out n,IntPtr.Zero)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error()); Array.Resize(ref data,(int)n); return data;}
  finally {pin.Free();}
 }
 public static int SetReparseDataError(SafeFileHandle h,byte[] data) {
  GCHandle pin=GCHandle.Alloc(data,GCHandleType.Pinned);
  try {uint n; return DeviceIoControl(h,0x000900A4u,pin.AddrOfPinnedObject(),(uint)data.Length,IntPtr.Zero,0,out n,IntPtr.Zero)?0:Marshal.GetLastWin32Error();}
  finally {pin.Free();}
 }
}
'@
function Remove-ProbeLink([string]$Path) {
 $h=[Wave5bHandlePoc]::Open($Path,$true,0x10080u,1)
 if(-not $h.IsInvalid){$d=$true;[void][Wave5bHandlePoc]::SetFileInformationByHandle($h,4,[ref]$d,1);$h.Dispose()}
}
function Invoke-VolumeProbe([string]$Volume,[byte[]]$Data) {
 $result=[ordered]@{volume=$Volume}
 $base=if($Volume -eq 'F:\'){$PSScriptRoot}else{[IO.Path]::GetTempPath()}
 $root=Join-Path $base ('handle-poc-work-'+$Volume.Substring(0,1)+'-'+[guid]::NewGuid().ToString('N'))
 $result.root=$root
 try {
  New-Item -ItemType Directory -Path $root | Out-Null
  $result.filesystem=([IO.DriveInfo]::new([IO.Path]::GetPathRoot([IO.Path]::GetFullPath($root)))).DriveFormat
  $target=Join-Path $root 'target'; New-Item -ItemType Directory -Path $target | Out-Null; [IO.File]::WriteAllText((Join-Path $target 'sentinel.txt'),'external')
  $pre=Join-Path $root 'preexisting-writer'; New-Item -ItemType Directory -Path $pre | Out-Null
  $writer=[Wave5bHandlePoc]::Open($pre,$true,0x40000000u,7); $guard=[Wave5bHandlePoc]::Open($pre,$true,0x10080u,1)
  $result.preexisting_writer_opened=(-not $writer.IsInvalid); $result.guard_rejected_existing_writer=$guard.IsInvalid
  if(-not $writer.IsInvalid){$writer.Dispose()}; if(-not $guard.IsInvalid){$guard.Dispose()}
  $locked=Join-Path $root 'locked'; New-Item -ItemType Directory -Path $locked | Out-Null
  $lh=[Wave5bHandlePoc]::Open($locked,$true,0x10080u,1)
  if($lh.IsInvalid){throw "guard-open-win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"}
  $fi=[Wave5bHandlePoc+Info]::new(); $result.info_ok=[Wave5bHandlePoc]::GetFileInformationByHandle($lh,[ref]$fi)
  $result.volume_serial=('{0:X8}' -f $fi.Volume); $result.file_index=('{0:X8}{1:X8}' -f $fi.IndexHi,$fi.IndexLow)
  try{Move-Item -LiteralPath $locked -Destination (Join-Path $root 'parked') -ErrorAction Stop;$result.rename_blocked=$false}catch{$result.rename_blocked=$true}
  $writer2=[Wave5bHandlePoc]::Open($locked,$true,0x40000000u,7); $result.write_handle_blocked=$writer2.IsInvalid; if(-not $writer2.IsInvalid){$writer2.Dispose()}
  $result.fsctl_on_guard_error=[Wave5bHandlePoc]::SetReparseDataError($lh,$Data); $lh.Dispose()
  $result.fsctl_on_guard_set_reparse=$false
  try{$result.fsctl_on_guard_set_reparse=([bool]((Get-Item -LiteralPath $locked -Force).Attributes -band [IO.FileAttributes]::ReparsePoint));$result.guard_link_target=(Get-Item -LiteralPath $locked -Force).Target}catch{}
  if($result.fsctl_on_guard_set_reparse){Remove-ProbeLink $locked}
  foreach($access in @(@{name='read-attributes';rights=0x80u},@{name='zero';rights=0u},@{name='generic-write';rights=0x40000000u})){
   $p=Join-Path $root ('ioctl-'+$access.name);New-Item -ItemType Directory -Path $p|Out-Null
   $h=[Wave5bHandlePoc]::Open($p,$true,[uint32]$access.rights,7)
   $key='fsctl_'+$access.name
   if($h.IsInvalid){$result[$key+'_open_error']=[Runtime.InteropServices.Marshal]::GetLastWin32Error();$result[$key+'_error']='OPEN-FAILED'}
   else{$result[$key+'_error']=[Wave5bHandlePoc]::SetReparseDataError($h,$Data);$h.Dispose()}
   try{$item=Get-Item -LiteralPath $p -Force;$result[$key+'_set_reparse']=([bool]($item.Attributes -band [IO.FileAttributes]::ReparsePoint));$result[$key+'_target']=$item.Target}catch{$result[$key+'_set_reparse']=$false}
   if($result[$key+'_set_reparse']){Remove-ProbeLink $p}
  }
  $file=Join-Path $root 'readonly.txt';[IO.File]::WriteAllText($file,'same-handle-delete');[IO.File]::SetAttributes($file,[IO.FileAttributes]::ReadOnly)
  $fh=[Wave5bHandlePoc]::Open($file,$false,0x10180u,1)
  $basic=[Wave5bHandlePoc+Basic]::new();$basic.Attr=0x80
  $result.readonly_basic_info_same_handle=[Wave5bHandlePoc]::SetFileInformationByHandle($fh,0,[ref]$basic,[Runtime.InteropServices.Marshal]::SizeOf($basic))
  $delete=$true;$result.file_disposition_same_handle=[Wave5bHandlePoc]::SetFileInformationByHandle($fh,4,[ref]$delete,1);$fh.Dispose()
  $result.readonly_file_exists_after_close=Test-Path $file
 }catch{$result.error=$_.Exception.Message}
 finally{if(Test-Path $root){Get-ChildItem -LiteralPath $root -Force -Recurse|ForEach-Object{if(-not $_.PSIsContainer){try{[IO.File]::SetAttributes($_.FullName,[IO.FileAttributes]::Normal)}catch{}}};Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue}}
 return [pscustomobject]$result
}
$sampleRoot=Join-Path ([IO.Path]::GetTempPath()) ('wave5b-junction-payload-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $sampleRoot | Out-Null
$target=Join-Path $sampleRoot 'target';New-Item -ItemType Directory -Path $target|Out-Null
$link=Join-Path $sampleRoot 'sample-link';New-Item -ItemType Junction -Path $link -Target $target|Out-Null
$sh=[Wave5bHandlePoc]::Open($link,$true,0x80u,1);$payload=[Wave5bHandlePoc]::GetReparseData($sh);$sh.Dispose()
try{@((Invoke-VolumeProbe 'F:\' $payload),(Invoke-VolumeProbe 'C:\' $payload))|ConvertTo-Json -Depth 5}
finally{Remove-ProbeLink $link;Remove-Item -LiteralPath $sampleRoot -Recurse -Force}
