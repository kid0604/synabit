const md = 'Here is text\n![demo](http://asset.localhost/%2FUsers%2Fkid0604%2FDesktop%2FProjects%2Fsynabit%2Fassets%2Fmy%20image.png)\nAnd more text';
const stripped = md.replace(/\]\((?:https?:\/\/asset\.localhost|asset:\/\/localhost)[^\)]+(?:\/|%2F)assets(?:\/|%2F)([^\)]+)\)/g, (_m, filename) => {
    return `](assets/${decodeURIComponent(filename)})`;
});
console.log(stripped);
