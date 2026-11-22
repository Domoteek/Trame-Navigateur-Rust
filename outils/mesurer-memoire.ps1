# mesurer-memoire.ps1 -- mesure le budget memoire reel d'une application et de ses descendants.
#
# Usage : powershell -NoProfile -File outils\mesurer-memoire.ps1 -Racine 1234 [-Etiquette "avant"]
#
# Note : ne jamais nommer une variable `$pid` en PowerShell, c'est une variable automatique
# en lecture seule (constate a la dure : boucle infinie).

param(
  [Parameter(Mandatory = $true)][int]$Racine,
  [string]$Etiquette = "mesure"
)

$src = @'
using System;
using System.Runtime.InteropServices;
public class Mem {
  [DllImport("psapi.dll")] public static extern bool EmptyWorkingSet(IntPtr h);
}
'@

$processus = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, WorkingSetSize, CommandLine
$parParent = @{}
foreach ($p in $processus) {
  $parent = [int]$p.ParentProcessId
  if (-not $parParent.ContainsKey($parent)) { $parParent[$parent] = @() }
  $parParent[$parent] += [int]$p.ProcessId
}

$vus = @{}
$file = [System.Collections.Generic.Queue[int]]::new()
$file.Enqueue($Racine)
$octets = 0L
$compte = 0
$detail = @{}

while ($file.Count -gt 0) {
  $courant = $file.Dequeue()
  if ($vus.ContainsKey($courant)) { continue }
  $vus[$courant] = $true

  $proc = $processus | Where-Object { $_.ProcessId -eq $courant }
  if ($proc) {
    $octets += [long]$proc.WorkingSetSize
    $compte++
    $nom = [string]$proc.Name
    if (-not $detail.ContainsKey($nom)) { $detail[$nom] = 0 }
    $detail[$nom]++
  }
  if ($parParent.ContainsKey($courant)) {
    foreach ($enfant in $parParent[$courant]) {
      if (-not $vus.ContainsKey($enfant)) { $file.Enqueue($enfant) }
    }
  }
}

$mo = [math]::Round($octets / 1MB, 1)
"{0} : {1} Mo sur {2} processus" -f $Etiquette, $mo, $compte
($detail.GetEnumerator() | Sort-Object -Property Value -Descending | ForEach-Object { "    {0} x{1}" -f $_.Key, $_.Value }) -join "`n"
