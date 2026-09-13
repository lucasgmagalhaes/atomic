//! Simulating user input against a live page: clicks (by selector or by
//! coordinate), focus/blur, typing, tabbing, and filling form-like
//! elements.
//!
//! Split into `helpers.rs` (small shared helpers), `focus.rs`
//! (focus/blur), `click.rs` (`dispatch_click`/`dispatch_click_at`),
//! `fill.rs` (`fill_element`), `keyboard.rs` (`type_key`), and `tab.rs`
//! (`TabOutcome`/`tab_focus`).

mod click;
mod fill;
mod focus;
mod helpers;
mod keyboard;
mod tab;

pub(crate) use click::{dispatch_click, dispatch_click_at};
pub(crate) use fill::fill_element;
pub(crate) use keyboard::type_key;
pub(crate) use tab::{tab_focus, TabOutcome};
