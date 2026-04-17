import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import ImageResize from 'tiptap-extension-resize-image';
import { JSDOM } from 'jsdom';

const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>');
global.document = dom.window.document;
global.window = dom.window;

const editor = new Editor({
  content: `Xuất phát từ yêu cầu các sếp:

<img src="asset://localhost/%2FUsers%2Fkid0604%2F.synabit%2Fgdrive-cache%2Fassets%2F1776421923-image.png" alt="Pasted Image" containerstyle="" wrapperstyle="display: flex">

Và đây:

<img src="asset://localhost/%2FUsers%2Fkid0604%2F.synabit%2Fgdrive-cache%2Fassets%2F1776421936-image.png" alt="Pasted Image" containerstyle="" wrapperstyle="display: flex">

Tiếp tục.`,
  extensions: [StarterKit, Markdown, ImageResize],
});

console.log(editor.storage.markdown.getMarkdown());
