import os
import re
import sys

def get_rel_path(file_path, base_path):
    # Rel path from file_path's dir to base_path
    dir_path = os.path.dirname(file_path)
    rel = os.path.relpath(base_path, dir_path)
    if not rel.startswith('.'):
        rel = './' + rel
    # Strip .ts extension
    if rel.endswith('.ts'):
        rel = rel[:-3]
    return rel

src_dir = '/Users/kid0604/Desktop/Projects/synabit/src'
logger_path = os.path.join(src_dir, 'utils', 'logger.ts')

modified = 0
for root, _, files in os.walk(src_dir):
    for f in files:
        if not f.endswith(('.vue', '.ts', '.js')): continue
        full_path = os.path.join(root, f)
        if full_path == logger_path: continue
        
        with open(full_path, 'r', encoding='utf-8') as file:
            content = file.read()
            
        if 'console.error' not in content and 'console.warn' not in content:
            continue
            
        # Needs replace
        content = content.replace('console.error', 'logger.error')
        content = content.replace('console.warn', 'logger.warn')
        
        # Inject import
        if 'import { logger }' not in content:
            rel_logger = get_rel_path(logger_path, logger_path) # wait, we need rel path from full_path to logger_path
            rel_logger = get_rel_path(logger_path, root) # wait, from dir of full_path to logger_path
            dir_path = os.path.dirname(full_path)
            rel = os.path.relpath(logger_path, dir_path)
            if not rel.startswith('.'):
                rel = './' + rel
            if rel.endswith('.ts'):
                rel = rel[:-3]
                
            import_statement = f"import {{ logger }} from '{rel}';\n"
            
            # Find last import in <script> or top level
            if '<script' in content:
                # Vue file
                script_end = content.find('</script>')
                last_import_idx = content.rfind('import ', 0, script_end)
                if last_import_idx != -1:
                    end_of_line = content.find('\n', last_import_idx)
                    content = content[:end_of_line+1] + import_statement + content[end_of_line+1:]
                else:
                    # No import, find script start
                    script_start = content.find('>', content.find('<script')) + 1
                    content = content[:script_start] + '\n' + import_statement + content[script_start:]
            else:
                # TS file
                last_import_idx = content.rfind('import ')
                if last_import_idx != -1:
                    end_of_line = content.find('\n', last_import_idx)
                    content = content[:end_of_line+1] + import_statement + content[end_of_line+1:]
                else:
                    content = import_statement + content
                    
        with open(full_path, 'w', encoding='utf-8') as file:
            file.write(content)
        modified += 1
        print(f"Modified {full_path}")

print(f"Total modified: {modified}")
