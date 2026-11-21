# W7-site-sticky-header

- Website: the header is now sticky on desktop and phone. It has a page-colour background (94 %, with a light blur)
  and shows a hairline and soft shadow only once the page has scrolled, via a tiny IntersectionObserver.
- In-page anchors land below it (`scroll-padding-top`). The language menu and theme button stay on top (header z 40,
  popover z 50, stage ≤ 3). There is no layout shift; the header stays in flow. The shadow transition is off under
  reduced motion.
