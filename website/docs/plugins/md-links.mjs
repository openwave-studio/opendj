// Lets the docs link to each other the way they read on GitHub, by file:
// [Looping](looping.md#loop-rolls). At build time each link to a .md file
// becomes the page's address (/docs/guide/looping.html#loop-rolls), worked
// out from the linking file's folder, so the same link works from any page,
// index pages included.
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const LOCAL_MD = /^(?![a-z][a-z0-9+.-]*:|\/|#)([^?#]*\.md)(#.*)?$/i;

export function pageUrl(target, fromDir, base) {
  const m = LOCAL_MD.exec(target);
  if (!m) return null;
  let page = path.posix.normalize(path.posix.join(fromDir, m[1]));
  page = page.replace(/\.md$/i, '').replace(/(^|\/)(index|README)$/i, '');
  if (page.startsWith('..')) return null;
  return base + (page ? page + '.html' : '') + (m[2] ?? '');
}

export default function mdLinks({ base, contentDir }) {
  const root = path.resolve(contentDir);
  return ({ fileURL }) => {
    if (!fileURL) return null;
    const file = fileURLToPath(fileURL);
    if (!file.startsWith(root + path.sep)) return null;
    const fromDir = path.relative(root, path.dirname(file)).split(path.sep).join('/');
    const fix = (node, ctx) => {
      const url = pageUrl(node.url ?? '', fromDir, base);
      if (url) ctx.setProperty(node, 'url', url);
    };
    return { name: 'opendj-md-links', link: fix, definition: fix };
  };
}
