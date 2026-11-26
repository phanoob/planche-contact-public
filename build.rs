// Icône et informations de l'exe Windows (Explorateur, barre des tâches, raccourcis).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Planche Contact")
            .set("FileDescription", "Planche Contact : planches A4 de magnets");
        res.compile().expect("ressources Windows");
    }
}
