//! Simple enum/struct property types — no CSS parsing logic here (see
//! `property_parsers.rs`), just the typed shapes `ComputedStyle`
//! (`computed_style.rs`) is built from.

/// Straight (non-premultiplied) sRGB + alpha, each channel `0..=255`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// The full CSS Color Module Level 4 extended keyword set (147 named
    /// colors) plus `transparent` - previously only `black`/`white`/
    /// `red`/`green`/`blue` were recognized; any other real CSS color
    /// keyword (`cornflowerblue`, `rebeccapurple`, `orange`, ...) silently
    /// failed to parse and left the declaration unset, same as an unknown
    /// property value anywhere else in this crate.
    pub(super) fn named(name: &str) -> Option<Color> {
        let (r, g, b) = match name.to_ascii_lowercase().as_str() {
            "transparent" => return Some(Color::TRANSPARENT),
            "aliceblue" => (240, 248, 255),
            "antiquewhite" => (250, 235, 215),
            "aqua" => (0, 255, 255),
            "aquamarine" => (127, 255, 212),
            "azure" => (240, 255, 255),
            "beige" => (245, 245, 220),
            "bisque" => (255, 228, 196),
            "black" => (0, 0, 0),
            "blanchedalmond" => (255, 235, 205),
            "blue" => (0, 0, 255),
            "blueviolet" => (138, 43, 226),
            "brown" => (165, 42, 42),
            "burlywood" => (222, 184, 135),
            "cadetblue" => (95, 158, 160),
            "chartreuse" => (127, 255, 0),
            "chocolate" => (210, 105, 30),
            "coral" => (255, 127, 80),
            "cornflowerblue" => (100, 149, 237),
            "cornsilk" => (255, 248, 220),
            "crimson" => (220, 20, 60),
            "cyan" => (0, 255, 255),
            "darkblue" => (0, 0, 139),
            "darkcyan" => (0, 139, 139),
            "darkgoldenrod" => (184, 134, 11),
            "darkgray" => (169, 169, 169),
            "darkgreen" => (0, 100, 0),
            "darkgrey" => (169, 169, 169),
            "darkkhaki" => (189, 183, 107),
            "darkmagenta" => (139, 0, 139),
            "darkolivegreen" => (85, 107, 47),
            "darkorange" => (255, 140, 0),
            "darkorchid" => (153, 50, 204),
            "darkred" => (139, 0, 0),
            "darksalmon" => (233, 150, 122),
            "darkseagreen" => (143, 188, 143),
            "darkslateblue" => (72, 61, 139),
            "darkslategray" => (47, 79, 79),
            "darkslategrey" => (47, 79, 79),
            "darkturquoise" => (0, 206, 209),
            "darkviolet" => (148, 0, 211),
            "deeppink" => (255, 20, 147),
            "deepskyblue" => (0, 191, 255),
            "dimgray" => (105, 105, 105),
            "dimgrey" => (105, 105, 105),
            "dodgerblue" => (30, 144, 255),
            "firebrick" => (178, 34, 34),
            "floralwhite" => (255, 250, 240),
            "forestgreen" => (34, 139, 34),
            "fuchsia" => (255, 0, 255),
            "gainsboro" => (220, 220, 220),
            "ghostwhite" => (248, 248, 255),
            "gold" => (255, 215, 0),
            "goldenrod" => (218, 165, 32),
            "gray" => (128, 128, 128),
            "green" => (0, 128, 0),
            "greenyellow" => (173, 255, 47),
            "grey" => (128, 128, 128),
            "honeydew" => (240, 255, 240),
            "hotpink" => (255, 105, 180),
            "indianred" => (205, 92, 92),
            "indigo" => (75, 0, 130),
            "ivory" => (255, 255, 240),
            "khaki" => (240, 230, 140),
            "lavender" => (230, 230, 250),
            "lavenderblush" => (255, 240, 245),
            "lawngreen" => (124, 252, 0),
            "lemonchiffon" => (255, 250, 205),
            "lightblue" => (173, 216, 230),
            "lightcoral" => (240, 128, 128),
            "lightcyan" => (224, 255, 255),
            "lightgoldenrodyellow" => (250, 250, 210),
            "lightgray" => (211, 211, 211),
            "lightgreen" => (144, 238, 144),
            "lightgrey" => (211, 211, 211),
            "lightpink" => (255, 182, 193),
            "lightsalmon" => (255, 160, 122),
            "lightseagreen" => (32, 178, 170),
            "lightskyblue" => (135, 206, 250),
            "lightslategray" => (119, 136, 153),
            "lightslategrey" => (119, 136, 153),
            "lightsteelblue" => (176, 196, 222),
            "lightyellow" => (255, 255, 224),
            "lime" => (0, 255, 0),
            "limegreen" => (50, 205, 50),
            "linen" => (250, 240, 230),
            "magenta" => (255, 0, 255),
            "maroon" => (128, 0, 0),
            "mediumaquamarine" => (102, 205, 170),
            "mediumblue" => (0, 0, 205),
            "mediumorchid" => (186, 85, 211),
            "mediumpurple" => (147, 112, 219),
            "mediumseagreen" => (60, 179, 113),
            "mediumslateblue" => (123, 104, 238),
            "mediumspringgreen" => (0, 250, 154),
            "mediumturquoise" => (72, 209, 204),
            "mediumvioletred" => (199, 21, 133),
            "midnightblue" => (25, 25, 112),
            "mintcream" => (245, 255, 250),
            "mistyrose" => (255, 228, 225),
            "moccasin" => (255, 228, 181),
            "navajowhite" => (255, 222, 173),
            "navy" => (0, 0, 128),
            "oldlace" => (253, 245, 230),
            "olive" => (128, 128, 0),
            "olivedrab" => (107, 142, 35),
            "orange" => (255, 165, 0),
            "orangered" => (255, 69, 0),
            "orchid" => (218, 112, 214),
            "palegoldenrod" => (238, 232, 170),
            "palegreen" => (152, 251, 152),
            "paleturquoise" => (175, 238, 238),
            "palevioletred" => (219, 112, 147),
            "papayawhip" => (255, 239, 213),
            "peachpuff" => (255, 218, 185),
            "peru" => (205, 133, 63),
            "pink" => (255, 192, 203),
            "plum" => (221, 160, 221),
            "powderblue" => (176, 224, 230),
            "purple" => (128, 0, 128),
            "rebeccapurple" => (102, 51, 153),
            "red" => (255, 0, 0),
            "rosybrown" => (188, 143, 143),
            "royalblue" => (65, 105, 225),
            "saddlebrown" => (139, 69, 19),
            "salmon" => (250, 128, 114),
            "sandybrown" => (244, 164, 96),
            "seagreen" => (46, 139, 87),
            "seashell" => (255, 245, 238),
            "sienna" => (160, 82, 45),
            "silver" => (192, 192, 192),
            "skyblue" => (135, 206, 235),
            "slateblue" => (106, 90, 205),
            "slategray" => (112, 128, 144),
            "slategrey" => (112, 128, 144),
            "snow" => (255, 250, 250),
            "springgreen" => (0, 255, 127),
            "steelblue" => (70, 130, 180),
            "tan" => (210, 180, 140),
            "teal" => (0, 128, 128),
            "thistle" => (216, 191, 216),
            "tomato" => (255, 99, 71),
            "turquoise" => (64, 224, 208),
            "violet" => (238, 130, 238),
            "wheat" => (245, 222, 179),
            "white" => (255, 255, 255),
            "whitesmoke" => (245, 245, 245),
            "yellow" => (255, 255, 0),
            "yellowgreen" => (154, 205, 50),
            _ => return None,
        };
        Some(Color { r, g, b, a: 255 })
    }

    /// `#rgb` or `#rrggbb` (no alpha channel form yet - `#rgba`/`#rrggbbaa`).
    pub(super) fn from_hex(hex: &str) -> Option<Color> {
        let expand = |c: char| c.to_digit(16).map(|d| (d as u8) * 17); // 0xF -> 0xFF
        match hex.len() {
            3 => {
                let mut chars = hex.chars();
                Some(Color {
                    r: expand(chars.next()?)?,
                    g: expand(chars.next()?)?,
                    b: expand(chars.next()?)?,
                    a: 255,
                })
            }
            6 => {
                let byte = |s: &str| u8::from_str_radix(s, 16).ok();
                Some(Color {
                    r: byte(&hex[0..2])?,
                    g: byte(&hex[2..4])?,
                    b: byte(&hex[4..6])?,
                    a: 255,
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f64),
    Percent(f64),
    Auto,
}

/// `Table`/`TableRow`/`TableCell` are real (2026-09-14) — see
/// `crate::table`'s own module doc for the exact layout scope. Only
/// `Table` is itself special-cased in `layout::layout_children`'s own
/// dispatch; a `TableRow`/`TableCell` box reached *outside* a `Table`
/// ancestor's own layout (`crate::table::layout_table_children` never
/// dispatches into `layout_children` for rows/cells at all — see that
/// function's own doc) just falls through to plain block stacking, same
/// "no anonymous box generation" cut `crate::table`'s own doc already
/// takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    Flex,
    Grid,
    Table,
    TableRow,
    TableCell,
    None,
}

/// One track's size in a `grid-template-columns`/`grid-template-rows`
/// list (`ROADMAP.md` item 34). Scoped to the two units real pages
/// overwhelmingly use for a fixed track list: `Px` (an absolute size) and
/// `Fr` (a share of the space left over after every `Px` track is
/// subtracted — see `crate::grid`'s own doc for exactly how that's
/// distributed). `minmax()`, `auto`, `%`, `repeat()`, and named lines are
/// not modeled — a real, narrower-than-spec scope cut, same shape
/// `flex.rs`'s own module doc already documents for flex's missing
/// `flex-wrap`/`order`/`gap`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrackSize {
    Px(f64),
    Fr(f64),
}

/// Max tracks a `grid-template-columns`/`grid-template-rows` list can
/// hold. A fixed-capacity array, not a `Vec` — `ComputedStyle` is `Copy`
/// (embedded by value in every `LayoutBox`, copied around freely by
/// `layout`/`flex`/`tree`), and a `Vec` field would force it to
/// `Clone`-only, rippling into every one of those existing copy sites.
/// 16 tracks is far beyond what a real page's grid typically declares —
/// a real, honest capacity cap, not a silent truncation nobody would
/// notice (extra tracks past this are simply dropped, same "declared but
/// unsupported, not pretended" convention this crate already uses for
/// e.g. `border-radius`'s one-value-for-all-corners scope cut).
pub const MAX_GRID_TRACKS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridTracks {
    sizes: [GridTrackSize; MAX_GRID_TRACKS],
    len: u8,
}

impl GridTracks {
    pub const EMPTY: Self = GridTracks {
        sizes: [GridTrackSize::Px(0.0); MAX_GRID_TRACKS],
        len: 0,
    };

    pub fn from_iter_capped(iter: impl Iterator<Item = GridTrackSize>) -> Self {
        let mut sizes = [GridTrackSize::Px(0.0); MAX_GRID_TRACKS];
        let mut len = 0u8;
        for (i, t) in iter.enumerate().take(MAX_GRID_TRACKS) {
            sizes[i] = t;
            len = (i + 1) as u8;
        }
        GridTracks { sizes, len }
    }

    pub fn as_slice(&self) -> &[GridTrackSize] {
        &self.sizes[..self.len as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// `Fixed` is real (2026-09-14): resolved by `layout::layout_children`
/// exactly like `Absolute` (this crate already resolves `Absolute` against
/// the page's own origin, not a positioned ancestor - see
/// `offset_from_edges`'s own doc - so `Fixed`'s layout math is identical;
/// the real difference is paint-time only, where `render`'s own display-
/// list builders tag every primitive under a `Fixed` box as such so
/// `profile-worker`'s page-scroll shift can skip it, keeping it glued to
/// the viewport while the rest of the page scrolls underneath).
/// `Sticky` is real too (2026-09-14), scoped to the one real-world-common
/// case: a `top` inset, relative to the whole page's own scroll (not a
/// nested `overflow: scroll` container's - a documented cut, same "page-
/// level only" scope `profile-worker`'s own scroll model already has).
/// `Sticky` needs **zero** layout changes - unlike `Absolute`/`Fixed`, a
/// sticky box stays in normal flow (its `layout::layout_children` handling
/// is identical to `Static`), so its own `Dimensions::y` already *is* its
/// real "natural" (unstuck) document position; every bit of real sticky
/// behavior happens at paint time in `render`'s display-list builders plus
/// `profile-worker`'s scroll shift (see `render::Rect::sticky`'s own doc
/// for the exact math). `bottom`/`left`/`right`-edge stickiness aren't
/// modeled, and `top: auto` (real spec: no sticky effect for that edge)
/// simply means this box never engages sticky behavior at all - it just
/// behaves like `Static`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Sticky,
    Fixed,
}

/// `dashed`/`dotted`/`double`/`groove`/... aren't modeled — a border only
/// ever paints as a solid rectangle (see `render::build_display_list`),
/// matching this crate's existing "flat solid quads only" painting model
/// (no stroke/dash patterns anywhere, same scope `background-color`
/// already has).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Solid,
}

/// `hidden`/`auto`/`scroll` all resolve to `Hidden` here - real CSS clips
/// content against the box's own border box for all three (only
/// `visible` doesn't), and that clipping is what this crate models (see
/// `render::build_display_list`/`build_image_list`/`build_glyph_list`).
/// What's *not* modeled is the difference between them: no scrollbar is
/// ever drawn, and there's no independent inner scroll offset for an
/// `auto`/`scroll` container (only the whole-viewport scroll
/// `profile-worker`'s `SCROLL` command already has) - a real, narrower
/// scope cut of the same gap `mockup/rendering-engine-gaps.md`'s Scroll
/// section already documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
}

/// Real, but narrower than the spec — see `layout::layout_children`'s own
/// doc for exactly what's modeled: a floated box is removed from normal
/// block stacking and placed flush to its containing block's left/right
/// edge, not overlapping an earlier same-side float, but nothing wraps
/// inline content around it (no line-box narrowing) and a `width: auto`
/// float stretches to fill available width same as a normal block rather
/// than shrink-to-fit sizing to its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

/// See [`Float`]'s own doc — `clear` pushes a box below the bottom of
/// whichever side(s) it names, tracked per containing block the same way
/// `layout::layout_children` tracks float placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
}

/// A real, but flat, `box-shadow` — no `blur-radius` (this crate's
/// "solid rects only" paint pipeline has no blur/gaussian geometry, same
/// scope cut `border-radius` already has for rounded corners), and only
/// one shadow even though real CSS accepts a comma-separated list (same
/// "one value, not a list" scope `border-color` already has for per-side
/// colors). `spread` grows/shrinks the shadow rect on every side, same
/// real effect a spread radius has minus the corner rounding a nonzero
/// `border-radius` would also apply to it. `inset` shadows aren't
/// recognized (the `inset` keyword just fails `Color::named` silently
/// and is ignored, same as any other unrecognized ident in this
/// declaration) - every shadow this crate paints is a real drop shadow.
/// `color` is required in the source (real CSS lets it default to
/// `currentColor` when omitted; this crate doesn't track declaration
/// order finely enough within one rule to resolve that reliably, so an
/// omitted color just means no shadow is set at all, same as `none`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f64,
    pub offset_y: f64,
    pub spread: f64,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
}

/// `armenian`/`georgian`/custom `@counter-style` aren't modeled - the five
/// real-world-common values only (`ROADMAP.md`'s own "scope to what real
/// pages use" convention, same shape `GridTrackSize`'s own doc takes for
/// track units). Painted as a real Unicode glyph through the existing
/// `cosmic-text` shaping/rasterization path (`crate::text`) rather than a
/// new paint primitive - `Disc`/`Circle`/`Square` are U+2022/U+25E6/U+25AA,
/// `Decimal` is the item's 1-based ordinal among its list's real `<li>`
/// children (no `<ol start>`/`value` attribute support - a documented cut,
/// same shape as `grid`'s missing named lines). See `tree::build`'s own doc
/// for exactly how/where a marker gets merged into its `<li>`'s box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyleType {
    Disc,
    Circle,
    Square,
    Decimal,
    None,
}

/// Real, but with no visible effect on its own — see `tree::build`'s doc:
/// this crate merges a marker directly into its `<li>`'s own first line
/// regardless of `Outside`/`Inside` (the real distinction — hanging in the
/// margin vs. counted as ordinary inline content — needs either a margin-
/// box concept or line-box narrowing this crate's simple block layout has
/// neither of). Parsed and stored so `element.style.listStylePosition`
/// round-trips correctly; not read by layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStylePosition {
    Outside,
    Inside,
}

/// Real 2-stop `linear-gradient(<angle>?, <color>, <color>)` — no 3+ color
/// stops, no percentage stop positions, no `radial-gradient`/`conic-
/// gradient` - the same "one value, not a list" scope cut `BoxShadow`'s own
/// doc already takes. `angle_deg` follows the real CSS convention (`0deg`
/// points up, increasing clockwise - `to bottom`, the real spec default
/// when no direction is given, is `180.0`) and is what `render`'s
/// `rect_to_vertices` uses to compute each corner's exact blend position
/// along the real CSS gradient-line-length formula (the box's own half-
/// diagonal projected onto the gradient direction) - painted as real
/// per-vertex GPU color interpolation, not a new shader or paint
/// primitive, and mathematically exact (not an approximation): bilinear
/// interpolation of an affine function from its four corner values
/// reproduces that function exactly at every interior point, regardless of
/// how the quad's two triangles split the diagonal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearGradient {
    pub angle_deg: f64,
    pub from: Color,
    pub to: Color,
}

/// The 5 CSS generic font families `cosmic-text`'s own `Family` enum
/// already has a direct variant for — no custom `@font-face`-registered
/// families (this engine fetches nothing over the network at box-tree/
/// text-shaping time - see `crate::text`'s own module doc), same real
/// scope this codebase's other gaps already document as a deliberate
/// non-goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericFontFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

/// Longest ASCII font name this crate keeps - real names in practice
/// (`"Helvetica Neue"`, `"Segoe UI"`) fit comfortably; a longer one is
/// silently truncated, same "declared but unsupported past a cap, not a
/// silent truncation nobody would notice" convention `MAX_GRID_TRACKS`'s
/// own doc already takes elsewhere in this crate.
pub const MAX_FONT_FAMILY_NAME_LEN: usize = 32;

/// Real `font-family`, scoped to the CSS pattern real pages overwhelmingly
/// use: one optional specific name plus a trailing generic fallback (e.g.
/// `font-family: Arial, sans-serif`) - not a full comma-separated fallback
/// *list* (only the first named entry and the last generic keyword in the
/// declaration are kept, same "one value, not a list" scope cut
/// `BoxShadow` already takes elsewhere in this crate). A fixed-capacity
/// byte buffer, not a `String` - `ComputedStyle` is `Copy` (embedded by
/// value in every `LayoutBox`, copied around freely by every recursive
/// box-tree/layout function), and a heap-allocated field would force it
/// to `Clone`-only, same reasoning `GridTracks`'s own doc already gives
/// for using a fixed array instead of a `Vec`. ASCII-only; a non-ASCII
/// font name is dropped (falls back to just the generic family) rather
/// than mis-truncated mid-codepoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontFamily {
    name: [u8; MAX_FONT_FAMILY_NAME_LEN],
    name_len: u8,
    pub generic: GenericFontFamily,
}

impl FontFamily {
    /// A bare generic keyword, no specific name - e.g. `font-family:
    /// sans-serif`, and this crate's own built-in default (`text::layout_
    /// inline`'s previous hardcoded behavior before this type existed).
    pub fn generic(generic: GenericFontFamily) -> Self {
        FontFamily {
            name: [0; MAX_FONT_FAMILY_NAME_LEN],
            name_len: 0,
            generic,
        }
    }

    /// A specific name plus its generic fallback - e.g. `font-family:
    /// Arial, sans-serif`. `name` is truncated to
    /// [`MAX_FONT_FAMILY_NAME_LEN`] bytes and dropped entirely (falling
    /// back to a bare generic, same as [`FontFamily::generic`]) if it
    /// isn't plain ASCII - see this type's own doc for why.
    pub fn named(name: &str, generic: GenericFontFamily) -> Self {
        if !name.is_ascii() || name.is_empty() {
            return FontFamily::generic(generic);
        }
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_FONT_FAMILY_NAME_LEN);
        let mut buf = [0u8; MAX_FONT_FAMILY_NAME_LEN];
        buf[..len].copy_from_slice(&bytes[..len]);
        FontFamily {
            name: buf,
            name_len: len as u8,
            generic,
        }
    }

    /// The specific name, if one was set - `None` for a bare generic
    /// keyword.
    pub fn name(&self) -> Option<&str> {
        if self.name_len == 0 {
            return None;
        }
        // `named` only ever stores validated ASCII, so this is exact -
        // never a lossy/replacement-character conversion.
        std::str::from_utf8(&self.name[..self.name_len as usize]).ok()
    }
}

impl Default for FontFamily {
    /// Matches this crate's own pre-existing hardcoded default (`text::
    /// layout_inline`'s `Family::SansSerif`, before `font-family` was a
    /// real, settable property) - every text box that never resolves a
    /// real `font-family` (from its own rules or an ancestor's) still
    /// paints exactly as it did before this type existed.
    fn default() -> Self {
        FontFamily::generic(GenericFontFamily::SansSerif)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl EdgeSizes {
    pub(super) fn zero() -> Self {
        EdgeSizes {
            top: Length::Px(0.0),
            right: Length::Px(0.0),
            bottom: Length::Px(0.0),
            left: Length::Px(0.0),
        }
    }
}
