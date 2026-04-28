import os
import re

directories = ["src-tauri/src/commands", "src-tauri/src/gdrive"]

for d in directories:
    for filename in os.listdir(d):
        if not filename.endswith(".rs"): continue
        filepath = os.path.join(d, filename)
        with open(filepath, "r") as f:
            content = f.read()

        lines = content.split('\n')
        changed = False

        for i in range(len(lines)):
            if "DbBridge::new(&vault_path)" in lines[i]:
                lines[i] = lines[i].replace("DbBridge::new(&vault_path)", "DbBridge::new(&app_handle)")
                changed = True
                
            # Now we must ensure the #[tauri::command] above it has app_handle
            if "#[tauri::command]" in lines[i]:
                # Find the fn line
                j = i + 1
                while j < len(lines) and "fn " not in lines[j]:
                    j += 1
                
                if j < len(lines):
                    # We only add app_handle if we are sure it's not there
                    if "app_handle" not in lines[j]:
                        # Let's check if the body of this function contains DbBridge::new(&vault_path)
                        # Find the end of the function body
                        k = j
                        brace_count = 0
                        body_has_db = False
                        
                        while k < len(lines):
                            if "{" in lines[k]:
                                brace_count += lines[k].count("{")
                            if "}" in lines[k]:
                                brace_count -= lines[k].count("}")
                            
                            if "DbBridge::new(&vault_path)" in lines[k]:
                                body_has_db = True
                                
                            if brace_count == 0 and k > j:
                                break
                            k += 1
                            
                        if body_has_db:
                            if "(" in lines[j] and ")" in lines[j]:
                                if lines[j].count("()") > 0:
                                    lines[j] = lines[j].replace("()", "(app_handle: tauri::AppHandle)")
                                else:
                                    lines[j] = lines[j].replace("(", "(app_handle: tauri::AppHandle, ")
                                changed = True

        if changed:
            with open(filepath, "w") as f:
                f.write('\n'.join(lines))

