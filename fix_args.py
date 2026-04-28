import os
import re

directories = ["src-tauri/src/commands", "src-tauri/src/gdrive"]

for d in directories:
    for filename in os.listdir(d):
        if not filename.endswith(".rs"): continue
        filepath = os.path.join(d, filename)
        with open(filepath, "r") as f:
            content = f.read()

        changed = False

        if "DbBridge::new(&vault_path)" in content:
            content = content.replace("DbBridge::new(&vault_path)", "DbBridge::new(&app_handle)")
            changed = True

        lines = content.split('\n')
        for i in range(len(lines)):
            if "#[tauri::command]" in lines[i]:
                j = i + 1
                while j < len(lines) and "fn " not in lines[j]:
                    j += 1
                if j < len(lines):
                    # Check if body uses app_handle
                    k = j
                    brace_count = 0
                    has_app_handle = False
                    while k < len(lines):
                        if "{" in lines[k]:
                            brace_count += lines[k].count("{")
                        if "}" in lines[k]:
                            brace_count -= lines[k].count("}")
                        if "&app_handle" in lines[k]:
                            has_app_handle = True
                        if brace_count == 0 and k > j:
                            break
                        k += 1

                    if has_app_handle and "app_handle: tauri::AppHandle" not in lines[j] and "app_handle: AppHandle" not in lines[j]:
                        lines[j] = re.sub(r'fn ([a-zA-Z0-9_]+)\(', r'fn \1(app_handle: tauri::AppHandle, ', lines[j])
                        lines[j] = lines[j].replace(", )", ")") # cleanup empty args
                        changed = True

        if changed:
            with open(filepath, "w") as f:
                f.write('\n'.join(lines))

