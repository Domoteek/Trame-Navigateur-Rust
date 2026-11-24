# Redimensionne la fenetre principale d'un Trame (cache ou non) SANS l'activer ni l'afficher.
# Sert aux bancs d'essai (outils/eprouver-menus.mjs) : un vrai WM_SIZE, comme quand on tire le
# bord de la fenetre, sans voler le focus a Clement.
#   powershell -File redimensionner-fenetre.ps1 -ProcessId 1234 -Largeur 1000 -Hauteur 600
param([int]$ProcessId, [int]$Largeur, [int]$Hauteur)

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class FenetreTrame {
  delegate bool Rappel(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumWindows(Rappel r, IntPtr p);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
  public static string Redimensionner(uint pid, int l, int h) {
    IntPtr trouvee = IntPtr.Zero;
    EnumWindows((w, p) => {
      uint proprio; GetWindowThreadProcessId(w, out proprio);
      if (proprio != pid || GetParent(w) != IntPtr.Zero) return true;
      var c = new StringBuilder(256); GetClassName(w, c, 256);
      // La fenetre de tao porte une classe a elle ; on ecarte les fenetres techniques.
      if (c.ToString().StartsWith("Window Class")) { trouvee = w; return false; }
      return true;
    }, IntPtr.Zero);
    if (trouvee == IntPtr.Zero) return "introuvable";
    double echelle = GetDpiForWindow(trouvee) / 96.0;
    // SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
    bool ok = SetWindowPos(trouvee, IntPtr.Zero, 0, 0, (int)(l * echelle), (int)(h * echelle), 0x0002 | 0x0004 | 0x0010);
    return ok ? "ok" : "refus";
  }
}
"@
[FenetreTrame]::Redimensionner([uint32]$ProcessId, $Largeur, $Hauteur)
