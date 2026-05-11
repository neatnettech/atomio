//! Theme tokens.
//!
//! Single source of truth for colors used in the gpui UI. Mirrors the CSS
//! variables in `docs/design/handoff/project/theme.css` so the live HTML
//! mock and the rendered app stay visually aligned.
//!
//! **Rule:** the UI crate must not introduce hex literals. Every colour
//! goes through this module. When the design spec changes, change tokens
//! here and the whole UI follows.
//!
//! Unused tokens are kept ahead of the UI catching up to the full design
//! spec; they're referenced from `docs/design.md` and will land as panes
//! get built. Dead-code lint is suppressed module-wide for that reason.

#![allow(dead_code)]

// ----- Surfaces (cool dark slate) -----
pub const BG_0: u32 = 0x0c0e11; // deepest, outside chrome
pub const BG_1: u32 = 0x11141a; // main app surface
pub const BG_2: u32 = 0x161a21; // sidebar / panels
pub const BG_3: u32 = 0x1c2129; // selected rows, hovered cards
pub const BG_4: u32 = 0x232934; // lifted elements, overlays
pub const BG_TOOLTIP: u32 = 0x2a3140;

// ----- Lines & dividers -----
// CSS uses rgba(255,255,255,0.05/0.08/0.14). Approximated as opaque on BG_1.
pub const LINE_1: u32 = 0x1a1d24;
pub const LINE_2: u32 = 0x21252d;
pub const LINE_3: u32 = 0x2e333d;

// ----- Text -----
pub const TX_1: u32 = 0xe8ebf0; // primary
pub const TX_2: u32 = 0xb6bdc9; // secondary
pub const TX_3: u32 = 0x7a8290; // tertiary / labels
pub const TX_4: u32 = 0x525866; // dim / placeholders
pub const TX_5: u32 = 0x363b46; // very dim — gutter inactive

// ----- Brand & semantic -----
pub const ACCENT: u32 = 0x3ecf8e; // run state, focus
pub const ACCENT_SOFT: u32 = 0x1c2a23; // accent on bg-1, ~14% alpha mix
pub const ACCENT_LINE: u32 = 0x256b4a; // accent border
pub const WARN: u32 = 0xffb454;
pub const WARN_SOFT: u32 = 0x2e2a1e;
pub const ERROR: u32 = 0xff6b6b;
pub const ERROR_SOFT: u32 = 0x2e1d1d;
pub const INFO: u32 = 0x6cb3ff;
pub const INFO_SOFT: u32 = 0x1d2630;
pub const MAGENTA: u32 = 0xd18cff;

// ----- Syntax (calm, Zed One Dark adjacent) -----
pub const SX_PL: u32 = 0xc8cfd9; // plain identifiers
pub const SX_KW: u32 = 0xd18cff; // keywords
pub const SX_FN: u32 = 0x6cb3ff; // functions
pub const SX_STR: u32 = 0x9bd87f; // strings
pub const SX_NUM: u32 = 0xffb454; // numbers
pub const SX_COMP: u32 = 0xff8a8a; // JSX components
pub const SX_PROP: u32 = 0xffd58a; // JSX props / attributes
pub const SX_TYPE: u32 = 0x6cb3ff; // types
pub const SX_COM: u32 = 0x4f5663; // comments
pub const SX_OP: u32 = 0x97a0ad; // operators
pub const SX_BRK: u32 = 0x7a8290; // brackets

// ----- Window chrome -----
pub const TRAFFIC_R: u32 = 0xff5f57;
pub const TRAFFIC_Y: u32 = 0xfebc2e;
pub const TRAFFIC_G: u32 = 0x28c941;

// Titlebar vertical gradient stops. Spec:
// `linear-gradient(180deg, #1a1f27 0%, #14181f 100%)`.
pub const TITLEBAR_TOP: u32 = 0x1a1f27;
pub const TITLEBAR_BOT: u32 = 0x14181f;
