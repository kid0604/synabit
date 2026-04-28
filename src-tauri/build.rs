fn main() {
    // Automatically load .env variables into the rustc environment at build time.
    // This allows env!("SYNABIT_GOOGLE_...") to work automatically during npm run tauri dev.
    println!("cargo:rerun-if-changed=.env");

    if let Ok(iter) = dotenvy::dotenv_iter() {
        for item in iter {
            if let Ok((key, val)) = item {
                println!("cargo:rustc-env={}={}", key, val);
            }
        }
    }

    tauri_build::build()
}
