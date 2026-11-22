//! Mesure du budget memoire reel de l'application.
//!
//! WebView2 cree un groupe de processus par dossier de profil : un processus navigateur
//! et un processus de rendu **par origine**, partages entre toutes les webviews de ce
//! profil. Il n'existe donc AUCUNE API donnant « la memoire de cet onglet » — pretendre le
//! contraire serait un mensonge d'interface. On mesure donc ce qui est mesurable : le
//! total de l'application (coquille Rust + tous ses descendants) et le nombre de
//! processus du groupe. C'est la seule grandeur honnête, et elle suffit a montrer l'effet
//! de la mise en veille.
//!
//! DEUX NIVEAUX DE LECTURE, et c'est ce qui rend la mesure peu couteuse :
//!
//! - un **balayage complet** (`ProcessesToUpdate::All`) enumere les ~400 processus de la
//!   machine pour decouvrir l'arbre ; il coutait l'essentiel du CPU de la coquille au repos
//!   (0,78 % d'un cœur mesures le 2026-11-21, surtout a cause de ce balayage toutes les 4 s) ;
//! - un **rafraichissement cible** ne relit que les PID deja connus, ce qui suffit pour suivre
//!   la memoire. Un balayage complet revient toutes les 5 mesures (20 s) pour decouvrir les
//!   nouveaux processus (une webview qui nait, un onglet reveille), et immediatement si le
//!   nombre de processus a baisse (un processus est mort : l'arbre a change).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Clone, Copy, Debug, Default)]
pub struct Mesure {
    pub mo: f64,
    pub processus: usize,
    /// Memoire des processus d'extension Chrome (`--extension-process`) de l'arbre.
    pub extensions_mo: f64,
    pub extensions_processus: usize,
    /// Memoire disponible sur la machine (Mo), pour adapter le nombre d'onglets ouverts.
    pub dispo_mo: f64,
}

/// Un processus de rendu d'extension Chrome : Chromium le marque `--extension-process`.
///
/// Constate le 2026-11-21 sur le profil reel : Bitwarden tourne dans un processus
/// `--type=renderer --extension-process --renderer-sub-type=extension` (184 Mo). La ligne de
/// commande ne dit PAS quelle extension il porte : on mesure donc le total exact, et le cout
/// d'une extension par difference au moment ou elle s'allume ou s'eteint.
pub fn est_processus_extension<S: AsRef<std::ffi::OsStr>>(ligne: &[S]) -> bool {
    ligne
        .iter()
        .any(|a| a.as_ref().to_string_lossy() == "--extension-process")
}

/// Une mesure sur combien de cycles on refait un balayage complet.
const PERIODE_BALAYAGE: u32 = 5;

struct Etat {
    systeme: System,
    /// Processus de l'arbre, racine comprise.
    suivis: Vec<Pid>,
    cycles: u32,
}

thread_local! {
    /// Le `System` est garde d'un appel a l'autre : sans cela, chaque mesure repartait d'une
    /// table vide et devait tout re-enumérer — c'est ce qui coutait cher.
    static ETAT: RefCell<Option<Etat>> = const { RefCell::new(None) };
}

/// Mesure le processus et tous ses descendants.
pub fn mesurer(pid_racine: u32) -> Mesure {
    let racine = Pid::from_u32(pid_racine);
    ETAT.with(|cellule| {
        let mut garde = cellule.borrow_mut();
        let etat = garde.get_or_insert_with(|| Etat {
            systeme: System::new(),
            suivis: vec![racine],
            // Le premier appel balaie forcement tout.
            cycles: PERIODE_BALAYAGE - 1,
        });

        // Balayage complet periodique, ou immediat si un processus a disparu.
        let disparu = etat.suivis.iter().any(|pid| etat.systeme.process(*pid).is_none());
        let balayer = etat.cycles + 1 >= PERIODE_BALAYAGE || disparu || etat.systeme.processes().is_empty();
        // La ligne de commande n'est lue qu'une fois par processus (`OnlyIfNotSet`) : elle ne
        // change pas, et c'est elle qui dit si c'est un processus d'extension.
        let genre = ProcessRefreshKind::nothing()
            .with_memory()
            .with_cmd(UpdateKind::OnlyIfNotSet);
        if balayer {
            etat.systeme
                .refresh_processes_specifics(ProcessesToUpdate::All, true, genre);
            etat.suivis = arbre(&etat.systeme, racine);
            etat.cycles = 0;
        } else {
            let liste = etat.suivis.clone();
            etat.systeme
                .refresh_processes_specifics(ProcessesToUpdate::Some(&liste), true, genre);
            etat.cycles += 1;
        }
        etat.systeme.refresh_memory();

        let mut octets = 0u64;
        let mut processus = 0usize;
        let mut octets_ext = 0u64;
        let mut processus_ext = 0usize;
        for pid in &etat.suivis {
            if let Some(p) = etat.systeme.process(*pid) {
                octets += p.memory();
                processus += 1;
                if est_processus_extension(p.cmd()) {
                    octets_ext += p.memory();
                    processus_ext += 1;
                }
            }
        }

        const MO: f64 = 1024.0 * 1024.0;
        Mesure {
            mo: octets as f64 / MO,
            processus,
            extensions_mo: octets_ext as f64 / MO,
            extensions_processus: processus_ext,
            dispo_mo: etat.systeme.available_memory() as f64 / MO,
        }
    })
}

/// Ensemble des processus de l'arbre (racine comprise), d'apres un balayage complet.
fn arbre(systeme: &System, racine: Pid) -> Vec<Pid> {
    let mut enfants: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, processus) in systeme.processes() {
        if let Some(parent) = processus.parent() {
            enfants.entry(parent).or_default().push(*pid);
        }
    }
    let mut vus: HashSet<Pid> = HashSet::new();
    let mut ordre = Vec::new();
    let mut pile = vec![racine];
    while let Some(pid) = pile.pop() {
        if !vus.insert(pid) {
            continue;
        }
        ordre.push(pid);
        if let Some(liste) = enfants.get(&pid) {
            pile.extend(liste.iter().copied());
        }
    }
    ordre
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_processus_d_extension_se_reconnait_a_sa_ligne_de_commande() {
        let ext = ["msedgewebview2.exe", "--type=renderer", "--extension-process", "--renderer-sub-type=extension"];
        assert!(est_processus_extension(&ext));
        let page = ["msedgewebview2.exe", "--type=renderer", "--renderer-client-id=7"];
        assert!(!est_processus_extension(&page));
        // Le navigateur porte `--embedded-browser-webview-enable-extension` : ce n'est pas un
        // processus d'extension.
        let navigateur = ["msedgewebview2.exe", "--embedded-browser-webview-enable-extension"];
        assert!(!est_processus_extension(&navigateur));
    }

    #[test]
    fn la_memoire_disponible_est_mesuree() {
        let m = mesurer(std::process::id());
        assert!(m.dispo_mo > 100.0, "memoire disponible : {:.0} Mo", m.dispo_mo);
        assert_eq!(m.extensions_processus, 0, "un test n'a pas de processus d'extension");
    }

    #[test]
    fn la_mesure_du_processus_courant_est_plausible() {
        // On se mesure soi-meme : au moins un processus, au moins un megaoctet.
        let mesure = mesurer(std::process::id());
        assert!(mesure.processus >= 1, "processus : {}", mesure.processus);
        assert!(mesure.mo > 1.0, "memoire : {:.1} Mo", mesure.mo);
    }

    #[test]
    fn plusieurs_mesures_consecutives_restent_coherentes() {
        // Les mesures ciblees (entre deux balayages) doivent rendre la meme chose a peu pres.
        let pid = std::process::id();
        let premiere = mesurer(pid);
        let mut dernieres = Vec::new();
        for _ in 0..7 {
            dernieres.push(mesurer(pid));
        }
        for m in &dernieres {
            assert_eq!(m.processus, premiere.processus, "le nombre de processus doit rester stable");
            assert!(
                (m.mo - premiere.mo).abs() < 200.0,
                "variation invraisemblable : {:.1} -> {:.1} Mo",
                premiere.mo,
                m.mo
            );
        }
    }
}
