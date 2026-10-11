import Slugger from 'github-slugger';

// Prefix the Chinese body before Astro collects headings and renders the TOC.
export function localizedAnchors({fileURL}) {
  if (!fileURL?.pathname.includes('/docs/zh/')) return null;
  const slugger = new Slugger();
  return {
    name: 'localized-anchors',
    element: {
      filter: ['h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'a'],
      visit(node, context) {
        if (/^h[1-6]$/.test(node.tagName)) {
          const id = node.properties?.id || slugger.slug(context.textContent(node));
          context.setProperty(node, 'id', `zh-${id}`);
        } else {
          const href = node.properties?.href;
          if (typeof href === 'string' && href.startsWith('#') && !href.startsWith('#zh-')) {
            context.setProperty(node, 'href', `#zh-${href.slice(1)}`);
          }
        }
      },
    },
  };
}
