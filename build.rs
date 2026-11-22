//! Integre l'icone et les metadonnees dans l'executable Windows.
//!
//! Sans cela, l'executable porte l'icone generique de Rust et Windows l'affiche telle quelle
//! dans l'explorateur, la barre des taches et l'alt-tab.

fn main() {
    println!("cargo:rerun-if-changed=ressources/trame.ico");

    #[cfg(windows)]
    {
        let mut ressources = winresource::WindowsResource::new();
        ressources.set_icon("ressources/trame.ico");
        ressources.set("ProductName", "Trame");
        ressources.set("FileDescription", "Trame — navigateur a onglets tisses");
        ressources.set("LegalCopyright", "MIT");
        match ressources.compile() {
            Ok(()) => println!("cargo:warning=icone integree a l'executable"),
            Err(erreur) => {
                // Une icone absente ne doit pas empecher de compiler : on le dit, et on continue.
                println!("cargo:warning=icone NON integree ({erreur})");
            }
        }
    }
}
