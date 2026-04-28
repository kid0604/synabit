import os

with open("src-tauri/src/commands/files.rs", "r") as f:
    files_rs = f.read()
files_rs = files_rs.replace("scan_directory(vault_path_clone", "scan_directory(app_handle.clone(), vault_path_clone")
files_rs = files_rs.replace("scan_directory(vault_path.clone()", "scan_directory(app_handle.clone(), vault_path.clone()")
with open("src-tauri/src/commands/files.rs", "w") as f:
    f.write(files_rs)

with open("src-tauri/src/commands/nexus.rs", "r") as f:
    nexus_rs = f.read()
nexus_rs = nexus_rs.replace("get_nexus_items(vault_path)", "get_nexus_items(app_handle.clone(), vault_path)")
with open("src-tauri/src/commands/nexus.rs", "w") as f:
    f.write(nexus_rs)
