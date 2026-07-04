# Welcome to Synabit Docs

This documentation is loaded **dynamically** at runtime from an external folder!

## How it works
If you map a Docker volume to the `public/content` directory, you can edit these `.md` files on your host machine or fetch them from a GitHub repository. The website will update instantly without needing a Docker rebuild.

## Example Code
```javascript
console.log("No Docker rebuilds required!");
```

### Try it out
Go to your `website/public/content/docs/getting-started.md` file, make a change, and just refresh the browser page.
