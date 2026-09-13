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
