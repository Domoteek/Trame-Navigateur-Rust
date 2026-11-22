//! Ponts vers Windows : telechargement HTTP, decompression d'une archive, choix d'un fichier.
//!
//! POURQUOI PASSER PAR POWERSHELL. Trame n'a aucune dependance reseau ni none (aucune crate
//! `reqwest`, `ureq` ou `zip` n'est dans le projet) : ajouter ces crates pour trois operations
//! ponctuelles changerait la nature du binaire — un navigateur qui embarque sa propre pile TLS
//! et son decompresseur ZIP pour installer une extension. Windows fournit deja les deux
//! (`HttpClient`, `Expand-Archive`), et le projet appelle deja `explorer` et `taskkill`.
//!
//! Ce que cela coute, dit franchement : ces trois fonctions sont **Windows seulement**, elles
//! ne sont pas couvertes par les tests unitaires (seulement par les scenarios de
//! `outils/verifier-app.mjs`), et une sortie inattendue de PowerShell doit produire un message
//! lisible plutot qu'une erreur muette. C'est le contrat de ce module : jamais de panique,
//! toujours un `Result` avec le motif.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

/// `CREATE_NO_WINDOW` : un outil en ligne de commande lance par Trame (application fenetree)
/// ouvrirait sinon sa propre console, visible et voleuse de focus. `-WindowStyle Hidden` ne
/// suffit pas : la console apparait avant que PowerShell ne la cache.
pub const SANS_CONSOLE: u32 = 0x0800_0000;

/// Execute un script PowerShell et rend (code, sortie complete).
fn powershell(script: &str) -> Result<(i32, String), String> {
    let sortie = Command::new("powershell")
        .creation_flags(SANS_CONSOLE)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .output()
        .map_err(|e| format!("PowerShell introuvable : {e}"))?;
    let mut texte = String::from_utf8_lossy(&sortie.stdout).to_string();
    texte.push_str(&String::from_utf8_lossy(&sortie.stderr));
    let code = sortie.status.code().unwrap_or(-1);
    Ok((code, texte.trim().to_string()))
}

/// Echappe une valeur pour l'inserer dans une chaine PowerShell entre apostrophes.
fn echapper(valeur: &str) -> String {
    valeur.replace('\'', "''")
}

/// Telecharge une URL dans un fichier. Rend le nombre d'octets ecrits.
///
/// Le magasin de Chrome repond **204 sans contenu** quand il refuse de servir (client trop
/// ancien, extension retiree) : c'est le cas d'erreur le plus frequent, et le message doit le
/// dire au lieu d'annoncer « fichier vide ».
pub fn telecharger(url: &str, cible: &Path) -> Result<u64, String> {
    if let Some(parent) = cible.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("dossier impossible : {e}"))?;
    }
    let script = format!(
        r#"Add-Type -AssemblyName System.Net.Http
$h = New-Object System.Net.Http.HttpClient
$h.Timeout = [TimeSpan]::FromSeconds(90)
$h.DefaultRequestHeaders.Add('User-Agent', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36')
try {{
  $rep = $h.GetAsync('{url}').Result
  $code = [int]$rep.StatusCode
  if ($code -ne 200) {{ Write-Output "CODE=$code"; exit 1 }}
  $octets = $rep.Content.ReadAsByteArrayAsync().Result
  [System.IO.File]::WriteAllBytes('{cible}', $octets)
  Write-Output "CODE=200"
  Write-Output "OCTETS=$($octets.Length)"
}} catch {{
  Write-Output ("ERREUR=" + $_.Exception.Message)
  exit 1
}}"#,
        url = echapper(url),
        cible = echapper(&cible.display().to_string())
    );
    let (code, sortie) = powershell(&script)?;
    if code != 0 {
        if let Some(reste) = sortie.split("CODE=").nth(1) {
            let numero: String = reste.chars().take_while(|c| c.is_ascii_digit()).collect();
            if numero == "204" {
                return Err(
                    "le magasin de Chrome a répondu 204 (rien à servir) : identifiant inconnu, \
                     extension retirée, ou client refusé"
                        .to_string(),
                );
            }
            if numero == "404" {
                return Err("extension introuvable sur le magasin (404)".to_string());
            }
            return Err(format!("le magasin a répondu {numero}"));
        }
        return Err(format!("téléchargement impossible : {sortie}"));
    }
    let octets = sortie
        .split("OCTETS=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if octets == 0 {
        return Err("le fichier téléchargé est vide".to_string());
    }
    Ok(octets)
}

/// Decompresse un ZIP dans un dossier (cree au besoin).
pub fn dezipper(zip: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|e| format!("dossier impossible : {e}"))?;
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
try {{
  Expand-Archive -LiteralPath '{zip}' -DestinationPath '{destination}' -Force
  Write-Output 'OK'
}} catch {{
  Write-Output ('ERREUR=' + $_.Exception.Message)
  exit 1
}}"#,
        zip = echapper(&zip.display().to_string()),
        destination = echapper(&destination.display().to_string())
    );
    let (code, sortie) = powershell(&script)?;
    if code != 0 || !sortie.contains("OK") {
        return Err(format!("décompression impossible : {sortie}"));
    }
    Ok(())
}

/// Ouvre le selecteur de fichier de Windows et rend le chemin choisi.
///
/// `None` = l'utilisateur a annule. C'est un cas normal, pas une erreur : l'interface ne doit
/// rien dire quand on ferme la boite de dialogue.
pub fn choisir_fichier(titre: &str, filtre: &str) -> Result<Option<String>, String> {
    let script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.OpenFileDialog
$d.Title = '{titre}'
$d.Filter = '{filtre}'
$d.CheckFileExists = $true
$d.Multiselect = $false
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ Write-Output $d.FileName }}"#,
        titre = echapper(titre),
        filtre = echapper(filtre)
    );
    let (code, sortie) = powershell(&script)?;
    if code != 0 {
        return Err(format!("sélecteur de fichier indisponible : {sortie}"));
    }
    let chemin = sortie.lines().last().unwrap_or("").trim().to_string();
    if chemin.is_empty() {
        Ok(None)
    } else {
        Ok(Some(chemin))
    }
}

/// Ouvre le selecteur « enregistrer sous » de Windows et rend le chemin choisi.
///
/// `nom_defaut` est propose dans la case, extension comprise ; l'utilisateur peut le changer.
/// `None` = annule (comme `choisir_fichier`), ce qui n'est pas une erreur.
pub fn choisir_enregistrement(
    titre: &str,
    filtre: &str,
    nom_defaut: &str,
) -> Result<Option<String>, String> {
    let script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.SaveFileDialog
$d.Title = '{titre}'
$d.Filter = '{filtre}'
$d.FileName = '{nom}'
$d.OverwritePrompt = $true
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ Write-Output $d.FileName }}"#,
        titre = echapper(titre),
        filtre = echapper(filtre),
        nom = echapper(nom_defaut)
    );
    let (code, sortie) = powershell(&script)?;
    if code != 0 {
        return Err(format!("sélecteur d'enregistrement indisponible : {sortie}"));
    }
    let chemin = sortie.lines().last().unwrap_or("").trim().to_string();
    Ok(if chemin.is_empty() { None } else { Some(chemin) })
}

/// Ouvre le selecteur de dossier de Windows et rend le dossier choisi (`None` : annule).
pub fn choisir_dossier(titre: &str) -> Result<Option<String>, String> {
    let script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.FolderBrowserDialog
$d.Description = '{titre}'
$d.UseDescriptionForTitle = $true
$d.ShowNewFolderButton = $true
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ Write-Output $d.SelectedPath }}"#,
        titre = echapper(titre)
    );
    let (code, sortie) = powershell(&script)?;
    if code != 0 {
        return Err(format!("sélecteur de dossier indisponible : {sortie}"));
    }
    let chemin = sortie.lines().last().unwrap_or("").trim().to_string();
    Ok(if chemin.is_empty() { None } else { Some(chemin) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_valeur_power_shell_est_echappee() {
        // Un chemin contenant une apostrophe casserait le script sans cet echappement.
        assert_eq!(echapper("C:\\un\\chemin"), "C:\\un\\chemin");
        assert_eq!(echapper("l'ete"), "l''ete");
    }
}
