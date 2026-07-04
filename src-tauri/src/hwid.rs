use blake3::Hasher;
use sysinfo::System;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn generate_hwid() -> String {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let mut hasher = Hasher::new();
    
    // CPU Information
    if let Some(cpu) = sys.cpus().first() {
        hasher.update(cpu.brand().as_bytes());
        hasher.update(cpu.vendor_id().as_bytes());
    }

    // Machine UID (immutable system ID like IOPlatformUUID or MachineGuid)
    let machine_id = machine_uid::get().unwrap_or_else(|_| "unknown-machine-id".to_string());
    hasher.update(machine_id.as_bytes());

    // OS info
    if let Some(os_name) = System::name() {
        hasher.update(os_name.as_bytes());
    }
    
    // Memory size can change, but MAC address is better.
    // However, sysinfo v0.30+ doesn't easily expose MAC addresses without a separate crate like `mac_address`.
    // Let's rely on machine_uid + blake3 hash
    
    let hash = hasher.finalize();
    let hwid_hex = hash.to_hex();
    
    // Return first 16 chars as a compact identifier
    format!("HWID-{}-{}", System::os_version().unwrap_or_else(|| "UNK".into()).replace(".", ""), &hwid_hex[..16]).to_uppercase()
}

#[cfg(target_os = "android")]
pub fn generate_hwid() -> String {
    // Placeholder for Android: e.g., Android Settings.Secure.ANDROID_ID
    "HWID-ANDROID-PLACEHOLDER".to_string()
}

#[cfg(target_os = "ios")]
pub fn generate_hwid() -> String {
    // Placeholder for iOS: e.g., UIDevice.current.identifierForVendor
    "HWID-IOS-PLACEHOLDER".to_string()
}

pub fn get_device_name() -> String {
    System::host_name().unwrap_or_else(|| "Unknown Device".to_string())
}
