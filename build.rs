fn main() {
    println!("cargo::rerun-if-changed=migrations");
    println!("cargo::rerun-if-changed=queries");
    built::write_built_file().expect("Failed to write build time information");
    // Force the application to be rebuilt after committing to ensure build info is up to date
    println!("cargo::rerun-if-changed=.git/refs");
}
