const shapes = {
  image: (
    <>
      <rect x="2" y="3" width="20" height="18" rx="2.5" />
      <circle cx="16.5" cy="8" r="1.5" />
      <path d="m4 17 5.5-6 5 5 3-3 3 3" />
    </>
  ),
  plus: <path d="M12 4v16M4 12h16" />,
  folder: <path d="M3 7V5a1 1 0 0 1 1-1h5l2 3h9a1 1 0 0 1 1 1v12H3V7Zm0 0h8" />,
  chevron: <path d="m9 5 7 7-7 7" />,
  globe: (
    <>
      <circle cx="12" cy="12" r="9" />
      <ellipse cx="12" cy="12" rx="4" ry="9" />
      <path d="M3 12h18M5 6h14M5 18h14" />
    </>
  ),
  close: <path d="m5 5 14 14M5 19 19 5" />,
  minimize: <path d="M5 12h14" />,
  maximize: <rect x="5" y="5" width="14" height="14" rx="1" />,
};
/** 与HTML原型共用的内联图形；含义由所属按钮或文字提供。 */
export function Icon({ name }: { name: keyof typeof shapes }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {shapes[name]}
    </svg>
  );
}
