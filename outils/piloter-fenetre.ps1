# pilote-fenetre.ps1 -- pilote une fenetre pour les tests d'interface de Trame.
#
# Pourquoi ce script existe : piloter une application a plusieurs webviews depuis
# l'exterieur est fragile (SendKeys bute sur les caracteres speciaux, et le premier plan
# change entre deux commandes, donc un clic part dans une autre fenetre). Ce script fait
# TOUT dans un seul processus : il remet la fenetre au premier plan, verifie ce qu'il y a
# sous le point vise, puis clique ou tape.
#
# Usage :
#   powershell -NoProfile -File outils\piloter-fenetre.ps1 -Focus 4132076 -Click 1209,246
#   powershell -NoProfile -File outils\piloter-fenetre.ps1 -Focus 4132076 -Texte "bonjour" -Entree
#   powershell -NoProfile -File outils\piloter-fenetre.ps1 -Focus 4132076 -Ctrl -Vk 0x20
#   powershell -NoProfile -File outils\piloter-fenetre.ps1 -Sonde 1209,246
#   powershell -NoProfile -File outils\piloter-fenetre.ps1 -Processus 32212 -Reduire
#   powershell -NoProfile -File outils\piloter-fenetre.ps1 -Processus 32212 -Restaurer
#
# Fichier volontairement en ASCII pur : un .ps1 UTF-8 sans BOM est illisible pour
# Windows PowerShell 5.1 (cf. AGENTS.md section 11).

param(
  [int]$Focus = 0,
  [int]$Processus = 0,
  [switch]$Reduire,
  [switch]$Restaurer,
  [string]$Click = "",
  [string]$Sonde = "",
  [string]$Texte = "",
  [switch]$Ctrl,
  [int]$Vk = 0,
  [switch]$Entree,
  [int]$Attente = 350
)

$source = @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public class Pilote {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] inputs, int size);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);

  public struct RECT { public int L, T, R, B; }

  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT { public uint type; public INPUTUNION u; }
  [StructLayout(LayoutKind.Explicit)]
  public struct INPUTUNION { [FieldOffset(0)] public KEYBDINPUT ki; }
  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }

  const uint INPUT_KEYBOARD = 1;
  const uint KEYEVENTF_KEYUP = 0x0002;
  const uint KEYEVENTF_UNICODE = 0x0004;

  static void ToucheBrute(ushort vk, bool relache) {
    var i = new INPUT();
    i.type = INPUT_KEYBOARD;
    i.u.ki.wVk = vk;
    i.u.ki.dwFlags = relache ? KEYEVENTF_KEYUP : 0;
    SendInput(1, new INPUT[] { i }, Marshal.SizeOf(typeof(INPUT)));
  }

  static void Caractere(char c) {
    var bas = new INPUT();
    bas.type = INPUT_KEYBOARD;
    bas.u.ki.wScan = c;
    bas.u.ki.dwFlags = KEYEVENTF_UNICODE;
    var haut = new INPUT();
    haut.type = INPUT_KEYBOARD;
    haut.u.ki.wScan = c;
    haut.u.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
    SendInput(2, new INPUT[] { bas, haut }, Marshal.SizeOf(typeof(INPUT)));
  }

  public static void Taper(string texte) {
    foreach (char c in texte) { Caractere(c); System.Threading.Thread.Sleep(8); }
  }

  public static void Touche(int vk, bool ctrl) {
    if (ctrl) ToucheBrute(0x11, false);           // VK_CONTROL
    ToucheBrute((ushort)vk, false);
    System.Threading.Thread.Sleep(30);
    ToucheBrute((ushort)vk, true);
    if (ctrl) ToucheBrute(0x11, true);
  }

  public static void Cliquer(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(60);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero);   // LEFTDOWN
    System.Threading.Thread.Sleep(40);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);   // LEFTUP
  }

  public static string Decrire(IntPtr h) {
    if (h == IntPtr.Zero) return "(nul)";
    var sb = new StringBuilder(256);
    GetClassName(h, sb, 256);
    uint pid;
    GetWindowThreadProcessId(h, out pid);
    RECT r;
    GetWindowRect(h, out r);
    return string.Format("{0} pid={1} rect=({2},{3},{4},{5})", sb, pid, r.L, r.T, r.R - r.L, r.B - r.T);
  }

  public static string SousPoint(int x, int y) {
    return Decrire(WindowFromPoint(new POINT { X = x, Y = y }));
  }

  public static string PremierPlan() { return Decrire(GetForegroundWindow()); }

  public static void Activer(IntPtr h) {
    ShowWindow(h, 9);            // SW_RESTORE
    SetForegroundWindow(h);
  }

  // WM_SYSCOMMAND / SC_MINIMIZE : exactement ce que fait le bouton de la barre des taches.
  public static void Reduire(IntPtr h) {
    PostMessage(h, 0x0112, (IntPtr)0xF020, IntPtr.Zero);
  }

  public static void Restaurer(IntPtr h) {
    ShowWindow(h, 9);            // SW_RESTORE
  }

  public static string Etat(IntPtr h) {
    if (h == IntPtr.Zero) return "fenetre=0";
    return "fenetre=" + h.ToString() + " minimisee=" + IsIconic(h);
  }
}
'@

Add-Type -TypeDefinition $source -Language CSharp

if ($Focus -ne 0) {
  [Pilote]::Activer([IntPtr]$Focus)
  Start-Sleep -Milliseconds $Attente
}

# Reduire / restaurer la fenetre d'un processus (banc d'essai du 2026-11-22 : reduire tuait
# Trame). L'etat est imprime apres l'action, pour que le test puisse le lire.
if ($Processus -ne 0) {
  $cible = Get-Process -Id $Processus -ErrorAction SilentlyContinue
  if ($null -eq $cible) { "processus $Processus introuvable"; exit 2 }
  $hw = [IntPtr]$cible.MainWindowHandle
  if ($hw -eq [IntPtr]::Zero) { "processus $Processus sans fenetre principale"; exit 3 }
  if ($Reduire) { [Pilote]::Reduire($hw) }
  if ($Restaurer) { [Pilote]::Restaurer($hw) }
  Start-Sleep -Milliseconds $Attente
  [Pilote]::Etat($hw)
}

if ($Sonde -ne "") {
  $p = $Sonde.Split(",")
  "premier plan : " + [Pilote]::PremierPlan()
  "sous le point : " + [Pilote]::SousPoint([int]$p[0], [int]$p[1])
}

if ($Click -ne "") {
  $p = $Click.Split(",")
  $x = [int]$p[0]; $y = [int]$p[1]
  "cible : " + [Pilote]::SousPoint($x, $y)
  [Pilote]::Cliquer($x, $y)
  Start-Sleep -Milliseconds 200
}

if ($Vk -ne 0) {
  [Pilote]::Touche($Vk, $Ctrl.IsPresent)
  Start-Sleep -Milliseconds 150
}

if ($Texte -ne "") {
  [Pilote]::Taper($Texte)
  Start-Sleep -Milliseconds 150
}

if ($Entree) {
  [Pilote]::Touche(0x0D, $false)
}
