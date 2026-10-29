fn main() {
    println!("cargo::rerun-if-changed=styles.css");
    println!("cargo::rerun-if-changed=src");
    println!("cargo::rerun-if-changed=../ui/theme/theme.css");
    topcoat::tailwind::BuildConfig::new()
        .version("4.3.3")
        .input("styles.css")
        .render()
        .expect("render the admin stylesheet; run `bun run generate` first to build the theme");
}
