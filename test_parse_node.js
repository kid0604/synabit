const { execSync } = require('child_process');
console.log("Checking tiptap-markdown package.json...", execSync('cat node_modules/tiptap-markdown/package.json | grep version').toString());
