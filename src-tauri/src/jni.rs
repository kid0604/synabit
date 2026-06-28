#[cfg(target_os = "android")]
use jni::objects::{JClass, JString};
#[cfg(target_os = "android")]
use jni::JNIEnv;

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_synabit_app_SyncWorker_runHeadlessSync(
    mut env: JNIEnv,
    _class: JClass,
    vault_path_jstring: JString,
    server_addr_jstring: JString,
    server_id_jstring: JString,
) {
    let vault_path: String = env
        .get_string(&vault_path_jstring)
        .expect("Couldn't get java string!")
        .into();
        
    let server_addr: String = env
        .get_string(&server_addr_jstring)
        .expect("Couldn't get java string!")
        .into();
        
    let server_id: String = env
        .get_string(&server_id_jstring)
        .expect("Couldn't get java string!")
        .into();

    println!("JNI: Starting headless sync for vault {}", vault_path);

    // Initialize Tokio runtime
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        // Here we would run the sync logic headless
        println!("JNI: Connecting to server {}", server_addr);
        
        let mut transport = crate::sync::server::SynabitServerTransport::new();
        if let Err(e) = transport.connect(&server_addr, &server_id).await {
            println!("JNI Error: failed to connect: {}", e);
            return;
        }
        
        println!("JNI: Connected successfully. Headless sync complete.");
        // TODO: Call engine::p2p_sync_full with a Headless context
        
        let _ = transport.disconnect().await;
    });
}
