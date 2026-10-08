/**
 * highlight.js's common grammars, in a module of their own so they can be a
 * chunk of their own. `lowlight` exposes only its root, so a deep dynamic
 * import of `lowlight/lib/common.js` is not available; re-exporting from here
 * is what lets the bundler leave the grammars out of the editor's load.
 */
export { common } from 'lowlight';
