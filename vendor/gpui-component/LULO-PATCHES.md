# lulo patches to gpui-component 0.9.1

Copied from crates.io and wired in with `[patch.crates-io]` in the workspace `Cargo.toml`.
Every change is marked with a `// lulo:` comment. Keep this list current, and redo the
changes on a fresh copy when upgrading.

- `src/popover.rs`: dropdowns open in 200 ms on ease-out quint (transitions.dev "Menu
  dropdown" curve, `cubic-bezier(0.22, 1, 0.36, 1)`), instead of 150 ms ease-out cubic.
- `src/popover.rs`: every `Popover` (so every `dropdown_menu`) fades and slides in like
  `dropdown_popup` does; upstream only animated selects, comboboxes and date pickers, so
  button menus appeared at once.
