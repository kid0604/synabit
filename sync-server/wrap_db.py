import re

with open('src/db.rs', 'r') as f:
    content = f.read()

content = content.replace('use r2d2::Pool;', 'use r2d2::Pool;\nuse tokio::task::block_in_place;')

def replacer(match):
    prefix = match.group(1)
    body = match.group(2)
    # Don't wrap open and migrate
    if 'pub fn open' in prefix or 'fn migrate' in prefix:
        return match.group(0)
    
    # Check if already wrapped
    if 'block_in_place' in body:
        return match.group(0)
    
    # Indent body
    lines = body.split('\n')
    indented_body = '\n'.join(['    ' + line if line.strip() else line for line in lines])
    
    wrapped = f"{prefix}{{\n        block_in_place(|| {{\n{indented_body}        }})\n    }}"
    return wrapped

content = re.sub(r'(pub fn [^{]+\{[ \n]+)(.*?\n    \})', replacer, content, flags=re.DOTALL)

with open('src/db.rs', 'w') as f:
    f.write(content)
