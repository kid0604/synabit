const regex = /!\[(.*?)\]\((.*?)\)/g;
const str = "![Image](assets/img-1776271761620-synabit-icon.png)";
console.log([...str.matchAll(regex)]);
