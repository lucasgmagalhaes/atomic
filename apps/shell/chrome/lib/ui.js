// Shared chrome UI kit — evaluated into every bundle's JS context before its
// own script runs (see `ChromeEngine::new`'s `UI_JS` eval). Plain function
// composition over the engine's real DOM API, not a virtual DOM / JSX
// runtime: every component here returns an actual DOM node built with
// `document.createElement`/`setAttribute`/`addEventListener` — the same
// calls a bundle would make by hand, just organized into reusable
// functions. No template parser, no build step: `Button({...}, '-')` is
// itself valid ES6, nothing needs translating before the engine runs it.
//
// `props` is always optional and `children` always variadic — `Div('hi')`,
// `Row({}, a, b, c)` and `Row({}, [a, b, c])` all work, so call sites never
// need to write `{}` for no attributes or `[...]` to pass more than one
// child. This is decided once here (`El`), every wrapper below inherits it
// for free by forwarding straight through.

// True only for a plain attributes object — not a string/number (a text
// child), an array (a children list), or a DOM node (a child element).
// This is what lets `El`/every wrapper below treat their first argument as
// either `props` or the first child, depending on what was actually passed.
function isProps(value) {
  return value != null && typeof value === "object" && !Array.isArray(value) && !(value instanceof Node);
}

// Flattens nested arrays (so `Row({}, [a, b], c)` and `Row({}, a, b, c)`
// behave the same — useful when a caller builds a children list with
// `.map()` and passes it alongside other children) and drops `null`/
// `undefined`/`false`, so a conditional child can be written
// `cond && Span('x')` directly in a children list.
function flattenChildren(children, out) {
  for (const child of children) {
    if (Array.isArray(child)) {
      flattenChildren(child, out);
    } else if (child != null && child !== false) {
      out.push(child);
    }
  }
  return out;
}

function toNode(child) {
  return typeof child === "string" || typeof child === "number" ? document.createTextNode(child) : child;
}

// The primitive every component below is a thin wrapper over: creates
// `tag`, applies `props` (an `on*` function becomes a real listener, not an
// attribute; `className` sets `.className`; anything else becomes an
// attribute unless it's `null`/`false`), and appends `children`.
function El(tag, propsOrChild, ...rest) {
  const props = isProps(propsOrChild) ? propsOrChild : {};
  const children = isProps(propsOrChild) ? rest : [propsOrChild, ...rest];

  const el = document.createElement(tag);
  for (const key in props) {
    const value = props[key];
    if (key.startsWith("on") && typeof value === "function") {
      el.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (key === "className") {
      el.className = value;
    } else if (value != null && value !== false) {
      el.setAttribute(key, value === true ? "" : value);
    }
  }
  flattenChildren(children, []).forEach((child) => el.appendChild(toNode(child)));
  return el;
}

// Builds a plain-tag wrapper (`Div`, `Span`, ...) that just forwards to
// `El` — every disambiguation rule above (optional props, variadic/nested
// children) comes along for free since `El` itself does the work.
function tag(name) {
  return (propsOrChild, ...rest) => El(name, propsOrChild, ...rest);
}

const Div = tag("div");
const Span = tag("span");
const Label = tag("label");
// Chrome surfaces don't navigate (no tabs, no history) — `href` is cosmetic
// here, real behavior comes from `onClick` like every other component.
const Link = tag("a");

// Headings, text/typography, and semantic containers — plain `El`
// wrappers, no special behavior. This engine has no default user-agent
// stylesheet (no built-in block/inline/list-item per tag), so these carry
// zero layout meaning of their own until a bundle's CSS styles them; they
// exist for naming/semantics and CSS-selector targeting (`h1 {}`, `pre
// {}`, ...), same as any of these tags would in a real page.
const P = tag("p");
const H1 = tag("h1");
const H2 = tag("h2");
const H3 = tag("h3");
const H4 = tag("h4");
const H5 = tag("h5");
const H6 = tag("h6");
const Strong = tag("strong");
const Em = tag("em");
const Small = tag("small");
const Pre = tag("pre");
const Code = tag("code");
const Section = tag("section");
const Header = tag("header");
const Footer = tag("footer");
const Nav = tag("nav");
const Article = tag("article");
const Aside = tag("aside");
const Main = tag("main");

// Lists.
const Ul = tag("ul");
const Ol = tag("ol");
const Li = tag("li");

// Tables.
const Table = tag("table");
const Thead = tag("thead");
const Tbody = tag("tbody");
const Tr = tag("tr");
const Td = tag("td");
const Th = tag("th");

// Form-adjacent tags without their own component logic (unlike `Button`/
// `Input`, which need id/active-class or self-closing handling) —
// `Select`/`Option` and `Textarea` still go through `El` like any other
// tag; `HTMLSelectElement`'s real `.value`/`.selectedIndex` (see the DOM
// capability matrix) work the same whether the `<option>`s were built by
// hand or through this wrapper.
const Select = tag("select");
const Option = tag("option");
const Textarea = tag("textarea");
const Fieldset = tag("fieldset");
const Legend = tag("legend");

// Void elements — no children, same shape `<input>`/`<img>` always have.
function Br(props) {
  return El("br", props || {});
}

function Hr(props) {
  return El("hr", props || {});
}

// `HTMLImageElement` has its own DOM subclass (see the DOM capability
// matrix) but no special construction needs here beyond being void.
function Img(props) {
  return El("img", props || {});
}

// `HTMLCanvasElement` has its own DOM subclass too — kept as a plain
// variadic wrapper (not void) since a `<canvas>` can carry fallback
// content children, same as a real page's markup.
const Canvas = tag("canvas");

// Self-closing — no children param, same shape `<input>` always has.
function Input(props) {
  return El("input", props || {});
}

// `props.active` toggles the shared `.btn.active` look (theme.css) — same
// highlight every bundle already used per-button (`ws-active`/`active`),
// now a single class name across all of them.
function Button(props, ...children) {
  return El(
    "button",
    { id: props.id, className: props.active ? "btn active" : "btn", onClick: props.onClick },
    ...children,
  );
}

function Row(props, ...children) {
  return El("div", { id: props && props.id, className: "row" }, ...children);
}

// An on/off button pair sharing one boolean — the `fps-cap-off`/`-on` and
// `keychain-off`/`-on` pattern `settings.js` used to hand-roll twice.
function Toggle(props) {
  return Row(
    {},
    Button({ id: props.offId, active: !props.isOn, onClick: props.onOff }, props.offLabel || "Off"),
    Button({ id: props.onId, active: props.isOn, onClick: props.onOn }, props.onLabel || "On"),
  );
}

// A list with a shared empty-state look (`.empty`, theme.css) — the
// adapter/credential/bookmark/downloads/history "either rows or a
// placeholder span" pattern `sync_settings_state`/`sync_downloads_history`
// used to build as escaped HTML strings.
function List(props) {
  if (!props.items.length) {
    return Span({ className: "empty" }, props.emptyText);
  }
  return Div(props.items.map((item, i) => props.renderItem(item, i)));
}

// Replaces `target`'s (an id string or a DOM node) children wholesale with
// `children` — the one call a bundle needs both to build its static chrome
// at load time and, inside a `render*` function, to refresh a dynamic
// region, instead of a chain of `.appendChild(...)` or a bare
// `.replaceChildren(...)` repeated at every call site.
function mount(target, ...children) {
  const root = typeof target === "string" ? document.getElementById(target) : target;
  root.replaceChildren(...flattenChildren(children, []).map(toNode));
}
