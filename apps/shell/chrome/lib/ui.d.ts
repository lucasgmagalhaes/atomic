// Ambient type declarations for `lib/ui.js`'s global component kit
// (El/Button/Row/Div/Span/Label/Input/Link/Toggle/List/mount) — same
// editor-only reasoning `../atomic.d.ts` documents: this workspace has no
// Node/npm build step, so nothing compiles or type-checks this file at
// build time. It exists purely so VS Code's TS language service can
// autocomplete and catch typos while editing the chrome bundle .js files
// (toolbar.js, settings.js, add_profile.js, downloads_history.js) that
// call these globals directly — picked up across the `chrome/` tree via
// `jsconfig.json` in the parent folder.
//
// Keep this in sync with `lib/ui.js` by hand — nothing enforces that
// automatically, same caveat `atomic.d.ts` carries for `chrome_bridge.rs`.

/** Any attribute/event-handler bag `El` (and every wrapper below) accepts. */
interface ElementProps {
  [attribute: string]: unknown;
  /** Sets `.id`. */
  id?: string;
  /** Sets `.className` directly (not merged with any tag-supplied class). */
  className?: string;
  /** Any `on<Event>` key (`onClick`, `onInput`, ...) becomes a real `addEventListener(<event>, ...)` call, not an attribute. */
  onClick?: (event?: Event) => void;
}

/**
 * A child accepted by `El`/every component below: a string or number
 * becomes a text node, `false`/`null`/`undefined` is dropped (so a
 * conditional child can be written `cond && Span('x')` inline), an array
 * is flattened arbitrarily deep (so a `.map()` result can be passed
 * alongside other children without wrapping it), and anything else is
 * assumed to already be a real DOM node.
 */
type Child = string | number | Node | false | null | undefined | Child[];

/** Shape shared by every plain-tag wrapper (`Div`, `P`, `Ul`, ...): optional `props`, variadic children. */
type PlainTag<E> = (propsOrChild?: ElementProps | Child, ...children: Child[]) => E;

/** Shape shared by every void-element wrapper (`Br`, `Hr`, `Img`): props only, no children. */
type VoidTag<E> = (props?: ElementProps) => E;

declare global {
  /**
   * The primitive every component below wraps: `document.createElement(tag)`
   * plus `props`/`children`. `propsOrChild` is optional — pass a plain
   * attributes object, or skip it and pass the first child directly
   * (`El('div', 'text')`); the rest of `children` is variadic either way.
   */
  function El(tag: string, propsOrChild?: ElementProps | Child, ...children: Child[]): HTMLElement;

  /**
   * `props.active` toggles the shared `.btn.active` look (theme.css) —
   * the same highlight every chrome bundle uses for a pressed/selected
   * button state.
   */
  function Button(props: ElementProps & { active?: boolean }, ...children: Child[]): HTMLButtonElement;

  /** A `<div class="row">` — the shared flex-row layout (theme.css). */
  function Row(props: ElementProps, ...children: Child[]): HTMLDivElement;

  /** Plain `<div>` — `props` is optional, same as `El`. */
  function Div(propsOrChild?: ElementProps | Child, ...children: Child[]): HTMLDivElement;

  /** Plain `<span>` — `props` is optional, same as `El`. */
  function Span(propsOrChild?: ElementProps | Child, ...children: Child[]): HTMLSpanElement;

  /** Plain `<label>` — `props` is optional, same as `El`. */
  function Label(propsOrChild?: ElementProps | Child, ...children: Child[]): HTMLLabelElement;

  /** Self-closing `<input>` — no children parameter. */
  function Input(props?: ElementProps): HTMLInputElement;

  /**
   * Plain `<a>` — chrome surfaces don't navigate (no tabs, no history),
   * so `href` is cosmetic; real behavior still comes from `onClick` like
   * every other component.
   */
  function Link(propsOrChild?: ElementProps | Child, ...children: Child[]): HTMLAnchorElement;

  // Headings, text/typography, and semantic containers — no default
  // block/inline/list-item styling of their own (this engine has no
  // user-agent stylesheet), just naming/semantics and CSS-selector
  // targeting like they'd carry in a real page.
  const P: PlainTag<HTMLParagraphElement>;
  const H1: PlainTag<HTMLHeadingElement>;
  const H2: PlainTag<HTMLHeadingElement>;
  const H3: PlainTag<HTMLHeadingElement>;
  const H4: PlainTag<HTMLHeadingElement>;
  const H5: PlainTag<HTMLHeadingElement>;
  const H6: PlainTag<HTMLHeadingElement>;
  const Strong: PlainTag<HTMLElement>;
  const Em: PlainTag<HTMLElement>;
  const Small: PlainTag<HTMLElement>;
  const Pre: PlainTag<HTMLPreElement>;
  const Code: PlainTag<HTMLElement>;
  const Section: PlainTag<HTMLElement>;
  const Header: PlainTag<HTMLElement>;
  const Footer: PlainTag<HTMLElement>;
  const Nav: PlainTag<HTMLElement>;
  const Article: PlainTag<HTMLElement>;
  const Aside: PlainTag<HTMLElement>;
  const Main: PlainTag<HTMLElement>;

  // Lists.
  const Ul: PlainTag<HTMLUListElement>;
  const Ol: PlainTag<HTMLOListElement>;
  const Li: PlainTag<HTMLLIElement>;

  // Tables.
  const Table: PlainTag<HTMLTableElement>;
  const Thead: PlainTag<HTMLElement>;
  const Tbody: PlainTag<HTMLElement>;
  const Tr: PlainTag<HTMLTableRowElement>;
  const Td: PlainTag<HTMLTableCellElement>;
  const Th: PlainTag<HTMLTableCellElement>;

  // Form-adjacent tags with no special component logic — `Select`'s real
  // `.value`/`.selectedIndex` (see the DOM capability matrix) work the
  // same whether the `<option>`s were built by hand or through `Option`.
  const Select: PlainTag<HTMLSelectElement>;
  const Option: PlainTag<HTMLOptionElement>;
  const Textarea: PlainTag<HTMLTextAreaElement>;
  const Fieldset: PlainTag<HTMLFieldSetElement>;
  const Legend: PlainTag<HTMLLegendElement>;

  /** Void — no children parameter. */
  const Br: VoidTag<HTMLBRElement>;
  /** Void — no children parameter. */
  const Hr: VoidTag<HTMLHRElement>;
  /** Void — no children parameter. `HTMLImageElement` has its own DOM subclass (see the DOM capability matrix). */
  const Img: VoidTag<HTMLImageElement>;

  /**
   * `HTMLCanvasElement` has its own DOM subclass too — kept variadic
   * (not void) since a `<canvas>` can carry fallback content children.
   */
  const Canvas: PlainTag<HTMLCanvasElement>;

  /** An on/off `Button` pair sharing one boolean state. */
  function Toggle(props: {
    offId: string;
    onId: string;
    isOn: boolean;
    offLabel?: string;
    onLabel?: string;
    onOff: () => void;
    onOn: () => void;
  }): HTMLDivElement;

  /**
   * A list with a shared empty-state look (`.empty`, theme.css) — either
   * `props.items.map(props.renderItem)` or, when `items` is empty, a
   * single `<span class="empty">` showing `props.emptyText`.
   */
  function List<T>(props: { items: T[]; emptyText?: string; renderItem?: (item: T, index: number) => Node }): HTMLElement;

  /**
   * Replaces `target`'s (an element id, or a DOM node) children wholesale
   * with `children` — the one call a bundle needs both to build its
   * static chrome at load time and, inside a `render*` function, to
   * refresh a dynamic region.
   */
  function mount(target: string | Node, ...children: Child[]): void;
}

export {};
